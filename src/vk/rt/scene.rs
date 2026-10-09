//! A scene on the stage: uploading triangles, materials and images, and building the
//! acceleration structures the ray-query tier traces (the compute tier walks the packed BVH).

use super::*;

impl RtStage {

    pub(super) fn write_image_descriptor(
        device: &ash::Device,
        set: vk::DescriptorSet,
        view: vk::ImageView,
        sampler: vk::Sampler,
    ) {
        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler_infos = [vk::DescriptorImageInfo::default().sampler(sampler)];
        unsafe {
            device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(7)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image_infos),
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(8)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler_infos),
                ],
                &[],
            );
        }
    }

    /// Whether this stage traverses a CPU-built BVH (tier 1) rather than
    /// building driver acceleration structures (tier 2).
    pub(crate) fn needs_bvh(&self) -> bool {
        self.tier == RtTier::Compute
    }

    /// Replace the scene with one already packed (`draw::rt::pack_scene`,
    /// whose image quad must be the one `image` describes). Tier 1 uploads
    /// its BVH, building it here only if it was packed without one; tier 2
    /// builds driver acceleration structures on the given queue instead,
    /// ignoring any BVH. Caller must have the device idle.
    pub(crate) fn set_scene(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        scene: &PackedScene,
        image: Option<(RtImageSource, [[f32; 3]; 4], f32)>,
    ) {
        if let Some(mut old) = self.image.take().and_then(|i| i.owned) {
            old.destroy(device, allocator);
        }
        self.image = image.map(|(source, corners, opacity)| match source {
            RtImageSource::Shared(id) => {
                StagedImage { shared: Some(id), owned: None, corners, opacity }
            }
            RtImageSource::Pixels { pixels, width, height } => StagedImage {
                shared: None,
                owned: Some(OwnedTexture::new(
                    device,
                    allocator,
                    queue,
                    command_pool,
                    pixels,
                    width,
                    height,
                )),
                corners,
                opacity,
            },
        });
        let scene = match self.tier {
            RtTier::Compute => scene.with_bvh(),
            RtTier::RayQuery => std::borrow::Cow::Borrowed(scene),
        };
        let (gpu_tris, gpu_mats, nodes) = (&scene.tris, &scene.materials, &scene.nodes);

        self.destroy_accel(device, allocator);
        for buf in [&mut self.nodes, &mut self.tris, &mut self.materials] {
            destroy_cpu_buffer(device, allocator, buf);
        }
        let upload = |allocator: &mut Allocator,
                      bytes: &[u8],
                      usage: vk::BufferUsageFlags,
                      name: &str|
         -> AllocatedBuffer {
            let mut buf = create_cpu_buffer(
                device,
                allocator,
                (bytes.len() as vk::DeviceSize).max(64),
                usage,
                name,
            );
            if !bytes.is_empty() {
                buf.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                    .copy_from_slice(bytes);
            }
            buf
        };
        // Tier 2 reads the same triangle buffer as BLAS build input (the
        // shading data still comes through the storage binding).
        let tri_usage = match self.tier {
            RtTier::Compute => vk::BufferUsageFlags::STORAGE_BUFFER,
            RtTier::RayQuery => {
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
            }
        };
        self.tris = upload(allocator, bytemuck::cast_slice(gpu_tris), tri_usage, "rt-tris");
        self.materials = upload(
            allocator,
            bytemuck::cast_slice(gpu_mats),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            "rt-materials",
        );
        self.tri_count = gpu_tris.len() as u32;
        self.sample_index = 0;

        // Binding 1 (per tier), then the shared 2/3.
        match self.tier {
            RtTier::Compute => {
                self.nodes = upload(
                    allocator,
                    bytemuck::cast_slice(nodes),
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                    "rt-nodes",
                );
                for frame in &self.frames {
                    let infos = [vk::DescriptorBufferInfo::default()
                        .buffer(self.nodes.buffer)
                        .range(vk::WHOLE_SIZE)];
                    unsafe {
                        device.update_descriptor_sets(
                            &[vk::WriteDescriptorSet::default()
                                .dst_set(frame.descriptor_set)
                                .dst_binding(1)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                                .buffer_info(&infos)],
                            &[],
                        );
                    }
                }
            }
            RtTier::RayQuery => {
                if self.tri_count > 0 {
                    self.build_accel(device, allocator, queue, command_pool);
                    let accel = self.accel.as_ref().unwrap();
                    let handles = [accel.tlas];
                    for frame in &self.frames {
                        let mut as_info =
                            vk::WriteDescriptorSetAccelerationStructureKHR::default()
                                .acceleration_structures(&handles);
                        let mut write = vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(1)
                            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                            .push_next(&mut as_info);
                        write.descriptor_count = 1;
                        unsafe { device.update_descriptor_sets(&[write], &[]) };
                    }
                }
            }
        }

        for frame in &self.frames {
            let infos = [
                vk::DescriptorBufferInfo::default().buffer(self.tris.buffer).range(vk::WHOLE_SIZE),
                vk::DescriptorBufferInfo::default()
                    .buffer(self.materials.buffer)
                    .range(vk::WHOLE_SIZE),
            ];
            let writes: Vec<vk::WriteDescriptorSet> = infos
                .iter()
                .enumerate()
                .map(|(i, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(frame.descriptor_set)
                        .dst_binding(2 + i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(info))
                })
                .collect();
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// Build the BLAS (over `self.tris`, opaque triangles) and a one-instance
    /// TLAS, on the given queue with a blocking one-time submit. Device is
    /// idle (set_scene contract), so replacing old structures is safe.
    pub(super) fn build_accel(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) {
        let loader = self.accel_loader.clone().expect("tier 2 without accel loader");
        let create_as_buffer = |allocator: &mut Allocator,
                                size: vk::DeviceSize,
                                usage: vk::BufferUsageFlags,
                                name: &str|
         -> AllocatedBuffer {
            unsafe {
                let buffer = device
                    .create_buffer(
                        &vk::BufferCreateInfo::default()
                            .size(size)
                            .usage(usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS)
                            .sharing_mode(vk::SharingMode::EXCLUSIVE),
                        None,
                    )
                    .expect("Failed to create AS buffer");
                let requirements = device.get_buffer_memory_requirements(buffer);
                let allocation = allocator
                    .allocate(&AllocationCreateDesc {
                        name,
                        requirements,
                        location: MemoryLocation::GpuOnly,
                        linear: true,
                        allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                    })
                    .expect("Failed to allocate AS memory");
                device
                    .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                    .expect("Failed to bind AS memory");
                AllocatedBuffer { buffer, allocation: Some(allocation), size }
            }
        };
        let addr_of = |buffer: vk::Buffer| unsafe {
            device.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(buffer))
        };

        unsafe {
            // --- BLAS over the triangle buffer (stride 16: p0/p1/p2 vec4s).
            let tri_addr = addr_of(self.tris.buffer);
            let blas_geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                        .vertex_format(vk::Format::R32G32B32_SFLOAT)
                        .vertex_data(vk::DeviceOrHostAddressConstKHR { device_address: tri_addr })
                        .vertex_stride(16)
                        .max_vertex(self.tri_count * 3 - 1)
                        .index_type(vk::IndexType::NONE_KHR),
                });
            let blas_geometries = [blas_geometry];
            let mut blas_build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(&blas_geometries);
            let blas_sizes = {
                let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
                loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &blas_build,
                    &[self.tri_count],
                    &mut sizes,
                );
                sizes
            };
            let blas_buffer = create_as_buffer(
                allocator,
                blas_sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
                "rt-blas",
            );
            let blas = loader
                .create_acceleration_structure(
                    &vk::AccelerationStructureCreateInfoKHR::default()
                        .buffer(blas_buffer.buffer)
                        .size(blas_sizes.acceleration_structure_size)
                        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL),
                    None,
                )
                .expect("Failed to create BLAS");

            // --- One-instance TLAS.
            let blas_addr = loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(blas),
            );
            let instance = vk::AccelerationStructureInstanceKHR {
                transform: vk::TransformMatrixKHR {
                    matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                },
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xff),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, 0),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas_addr,
                },
            };
            let instance_bytes = std::slice::from_raw_parts(
                (&instance as *const vk::AccelerationStructureInstanceKHR).cast::<u8>(),
                std::mem::size_of::<vk::AccelerationStructureInstanceKHR>(),
            );
            let mut instances = create_cpu_buffer(
                device,
                allocator,
                instance_bytes.len() as vk::DeviceSize,
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                "rt-tlas-instances",
            );
            instances.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
                [..instance_bytes.len()]
                .copy_from_slice(instance_bytes);

            let tlas_geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::INSTANCES)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                        .array_of_pointers(false)
                        .data(vk::DeviceOrHostAddressConstKHR {
                            device_address: addr_of(instances.buffer),
                        }),
                });
            let tlas_geometries = [tlas_geometry];
            let mut tlas_build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(&tlas_geometries);
            let tlas_sizes = {
                let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
                loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &tlas_build,
                    &[1],
                    &mut sizes,
                );
                sizes
            };
            let tlas_buffer = create_as_buffer(
                allocator,
                tlas_sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
                "rt-tlas",
            );
            let tlas = loader
                .create_acceleration_structure(
                    &vk::AccelerationStructureCreateInfoKHR::default()
                        .buffer(tlas_buffer.buffer)
                        .size(tlas_sizes.acceleration_structure_size)
                        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL),
                    None,
                )
                .expect("Failed to create TLAS");

            // Shared scratch, aligned to the device's scratch requirement
            // (buffer device addresses only guarantee allocation alignment).
            let scratch_size =
                blas_sizes.build_scratch_size.max(tlas_sizes.build_scratch_size);
            let mut scratch = create_as_buffer(
                allocator,
                scratch_size + self.as_scratch_align,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                "rt-as-scratch",
            );
            let scratch_addr =
                addr_of(scratch.buffer).next_multiple_of(self.as_scratch_align.max(1));

            blas_build = blas_build
                .dst_acceleration_structure(blas)
                .scratch_data(vk::DeviceOrHostAddressKHR { device_address: scratch_addr });
            tlas_build = tlas_build
                .dst_acceleration_structure(tlas)
                .scratch_data(vk::DeviceOrHostAddressKHR { device_address: scratch_addr });

            // One-time submit: BLAS build → barrier → TLAS build.
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate AS build command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            let blas_range = [vk::AccelerationStructureBuildRangeInfoKHR::default()
                .primitive_count(self.tri_count)];
            loader.cmd_build_acceleration_structures(cmd, &[blas_build], &[&blas_range]);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                    .dst_access_mask(
                        vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR
                            | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                    )],
                &[],
                &[],
            );
            let tlas_range =
                [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(1)];
            loader.cmd_build_acceleration_structures(cmd, &[tlas_build], &[&tlas_range]);
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&cmds)],
                    vk::Fence::null(),
                )
                .expect("AS build submit failed");
            let _ = device.queue_wait_idle(queue);
            device.free_command_buffers(command_pool, &cmds);
            destroy_cpu_buffer(device, allocator, &mut scratch);

            self.accel = Some(Accel { blas, blas_buffer, tlas, tlas_buffer, instances });
        }
    }

    pub(super) fn destroy_accel(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        if let Some(mut accel) = self.accel.take() {
            let loader = self.accel_loader.as_ref().expect("accel without loader");
            unsafe {
                loader.destroy_acceleration_structure(accel.tlas, None);
                loader.destroy_acceleration_structure(accel.blas, None);
            }
            destroy_cpu_buffer(device, allocator, &mut accel.tlas_buffer);
            destroy_cpu_buffer(device, allocator, &mut accel.blas_buffer);
            destroy_cpu_buffer(device, allocator, &mut accel.instances);
        }
    }
}
