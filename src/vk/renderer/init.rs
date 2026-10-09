//! Construction: instance, surface, device and queues, the swapchain, pipelines and stages.

use super::*;

impl VkRenderer {
    /// A renderer presenting to `surface_ptr`, or [`SurfaceLost`] when the
    /// display connection under it is already dead — which is what a window
    /// requested as the compositor goes away gets. The caller should treat
    /// that as its connection ending (the runner does), not retry here.
    ///
    /// # Safety
    /// `display_ptr` and `surface_ptr` must be live `wl_display` / `wl_surface`
    /// pointers that outlive the renderer.
    pub unsafe fn try_new(
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
        width: u32,
        height: u32,
        corner_radius_px: f32,
    ) -> Result<Self, SurfaceLost> {
        Self::try_new_for(
            crate::vk::core::SurfaceTarget::Wayland { display: display_ptr, surface: surface_ptr },
            width,
            height,
            corner_radius_px,
        )
    }

    /// A renderer presenting to any window [`SurfaceTarget`](crate::vk::core::SurfaceTarget)
    /// names — a Wayland surface, or on macOS a `CAMetalLayer` — on the same
    /// terms as [`try_new`](Self::try_new).
    ///
    /// # Safety
    /// The target's pointers must be live and outlive the renderer.
    pub unsafe fn try_new_for(
        target: crate::vk::core::SurfaceTarget,
        width: u32,
        height: u32,
        corner_radius_px: f32,
    ) -> Result<Self, SurfaceLost> {
        let t_new = std::time::Instant::now();
        let (mut core, surface) = crate::vk::core::VkCore::new_for_surface(target)?;
        log::debug!("[timing] VkCore::new_for_surface: {:?}", t_new.elapsed());
        let t_rest = std::time::Instant::now();
        // Locals over the core for the setup below (methods use self.core.*).
        let device = core.device.clone();
        let queue = core.queue;
        let command_pool = core.command_pool;
        let physical_device = core.physical_device;
        let min_uniform_align = core.min_uniform_align;
        let surface_loader = core.surface_loader.clone();

        // Surface format: prefer sRGB (wgpu's get_default_config sorts sRGB first,
        // so this matches the colors the app renders today). The first query
        // that talks to the compositor, so the one a dead connection fails.
        let formats = match surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
        {
            Ok(formats) if !formats.is_empty() => formats,
            Ok(_) => panic!("surface offers no formats"),
            Err(result) => {
                surface_loader.destroy_surface(surface, None);
                return Err(SurfaceLost { call: "vkGetPhysicalDeviceSurfaceFormatsKHR", result });
            }
        };
        let allocator = core.allocator.as_mut().unwrap();
        let surface_format = formats
            .iter()
            .copied()
            .find(|f| {
                (f.format == vk::Format::B8G8R8A8_SRGB || f.format == vk::Format::R8G8B8A8_SRGB)
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(formats[0]);

        // Render pass: one color attachment, clear -> present.
        let attachments = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let color_refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)];
        // One dependency shared VERBATIM by both UI pass variants: framebuffer
        // compatibility requires identical dependencies (only load/store ops and
        // image layouts may differ), so this unions the clear case (previous
        // frame's color output) with the load case (the backdrop copy's write).
        let dependencies = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::TRANSFER,
            )
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )];
        let render_pass = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
            .expect("Failed to create render pass");

        // Variant used when a backdrop copy precedes the UI pass: keep the copied
        // pixels (LOAD) and take the image from the copy's TRANSFER_DST layout.
        let attachments_load = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let render_pass_load = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments_load)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
            .expect("Failed to create load render pass");

        // Variant for a partial frame: keep what the image already shows and
        // repaint inside the damage only. A presented image comes back from
        // acquire in PRESENT_SRC with its contents intact.
        let attachments_partial = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let render_pass_partial = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments_partial)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
            .expect("Failed to create partial render pass");

        let (descriptor_set_layout, pipeline_layout, shader_module, pipeline) =
            create_ui_pipeline(&device, render_pass);

        // Full-size backdrop + depth live in the scene stage: the 3D pass renders
        // into the backdrop, and the UI pass samples it for blur-behind plates.
        let initial_extent = vk::Extent2D { width: width.max(1), height: height.max(1) };
        let scene = SceneStage::new(
            &device,
            allocator,
            surface_format.format,
            initial_extent,
            FRAMES_IN_FLIGHT,
            min_uniform_align,
            core.max_line_width,
        );
        clear_image_to_shader_read(&device, queue, command_pool, scene.backdrop_image);

        // Linear, clamp-to-edge, and linear BETWEEN mip levels: the blur
        // snapshot carries a mip chain and the frost kernel reads it at a
        // fractional level (shader2d's `resolve_blur`). The scene backdrop has
        // one level, which every level clamps to.
        let backdrop_sampler = device
            .create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .max_lod(vk::LOD_CLAMP_NONE)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )
            .expect("Failed to create sampler");

        let window_info = create_cpu_buffer(
            &device,
            allocator,
            WINDOW_INFO_BYTES,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            "window-info",
        );
        // Plate-carve features, one MAX_PLATE_FEATURES slot per frame in
        // flight so a write never races the previous frame's reads.
        let plate_features = create_cpu_buffer(
            &device,
            allocator,
            (FRAMES_IN_FLIGHT * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES) as vk::DeviceSize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            "plate-features",
        );

        // Two sets: the scene-backdrop set and its snapshot twin (binding 0
        // differs; 1-3 alias the same sampler/uniforms).
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(4),
        ];
        let descriptor_pool = device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(2)
                    .pool_sizes(&pool_sizes),
                None,
            )
            .expect("Failed to create descriptor pool");
        let both_layouts = [descriptor_set_layout, descriptor_set_layout];
        let sets = device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&both_layouts),
            )
            .expect("Failed to allocate descriptor sets");
        let (descriptor_set, descriptor_set_snapshot) = (sets[0], sets[1]);

        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(scene.backdrop_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler_infos = [vk::DescriptorImageInfo::default().sampler(backdrop_sampler)];
        let buffer_infos = [vk::DescriptorBufferInfo::default()
            .buffer(window_info.buffer)
            .offset(0)
            .range(WINDOW_INFO_BYTES)];
        let feature_infos = [vk::DescriptorBufferInfo::default()
            .buffer(plate_features.buffer)
            .offset(0)
            .range((FRAMES_IN_FLIGHT * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES) as vk::DeviceSize)];
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&feature_infos),
                // Snapshot twin: bindings 1-3 alias the same objects; binding 0
                // is written by `sync_backdrop_targets` once the snapshot image
                // exists.
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set_snapshot)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set_snapshot)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set_snapshot)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&feature_infos),
            ],
            &[],
        );

        // Per-frame command buffers, sync, and vertex buffers.
        let cmds = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(FRAMES_IN_FLIGHT as u32),
            )
            .expect("Failed to allocate command buffers");
        let frames = cmds
            .into_iter()
            .map(|cmd| Frame {
                cmd,
                image_available: device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .unwrap(),
                in_flight: device
                    .create_fence(
                        &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                        None,
                    )
                    .unwrap(),
                vertex: create_cpu_buffer(
                    &device,
                    allocator,
                    64 * 1024,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    "vertices",
                ),
                vertex_count: 0,
                overlay_start: 0,
                overlay_count: 0,
            })
            .collect();

        let text = TextStage::new(&device, allocator, render_pass, FRAMES_IN_FLIGHT);
        // Whether a mip chain can be built by blitting: both of the formats a
        // user image may be in have to be a blit's source and destination
        // and filter linearly. Asked here, where the instance is.
        let mips_supported = [vk::Format::R8G8B8A8_SRGB, vk::Format::B8G8R8A8_SRGB]
            .into_iter()
            .all(|format| {
                let needed = vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
                unsafe {
                    core.instance
                        .get_physical_device_format_properties(core.physical_device, format)
                }
                .optimal_tiling_features
                .contains(needed)
            });
        let blur_mips = {
            let needed = vk::FormatFeatureFlags::BLIT_SRC
                | vk::FormatFeatureFlags::BLIT_DST
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
            unsafe {
                core.instance
                    .get_physical_device_format_properties(core.physical_device, surface_format.format)
            }
            .optimal_tiling_features
            .contains(needed)
        };
        let image = ImageStage::new(
            &device,
            allocator,
            render_pass,
            FRAMES_IN_FLIGHT,
            mips_supported,
            core.max_anisotropy,
        );

        let swapchain_loader = ash::khr::swapchain::Device::new(&core.instance, &device);
        let mut renderer = Self {
            surface,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            surface_format,
            extent: vk::Extent2D { width: width.max(1), height: height.max(1) },
            swapchain_views: Vec::new(),
            framebuffers: Vec::new(),
            render_finished: Vec::new(),
            render_pass,
            render_pass_load,
            render_pass_partial,
            image_ages: Vec::new(),
            descriptor_set_layout,
            pipeline_layout,
            pipeline,
            shader_module,
            descriptor_pool,
            descriptor_set,
            descriptor_set_snapshot,
            snapshot_image: vk::Image::null(),
            snapshot_view: vk::ImageView::null(),
            snapshot_allocation: None,
            snapshot_wanted: false,
            snapshot_levels: 1,
            blur_mips,
            minimal_swapchain: false,
            backdrop_sampler,
            window_info,
            profile_gen: 0,
            relief_uploaded: (0.0, 0.0),
            roll_profile_gen: 0,
            plate_features,
            frames,
            frame_index: 0,
            text,
            scene,
            image,
            rt: None,
            rt_background: None,
            rt_environment: RtEnvironment::default(),
            desired_extent: vk::Extent2D { width: width.max(1), height: height.max(1) },
            corner_radius_px,
            swapchain_dirty: false,
            present_mode: vk::PresentModeKHR::FIFO,
            present_debug_count: 0,
            surface_lost: false,
            core,
        };
        log::debug!("[timing] VkRenderer pipelines/stages: {:?}", t_rest.elapsed());
        let t_swap = std::time::Instant::now();
        // On failure `renderer` drops here, and its Drop tears down everything
        // built so far, surface included.
        renderer.create_swapchain()?;
        renderer.write_window_info();
        // The swapchain may have settled on a different extent than requested;
        // keep the backdrop targets in lockstep.
        renderer.sync_backdrop_targets();
        log::debug!("[timing] swapchain setup: {:?}", t_swap.elapsed());
        Ok(renderer)
    }
}
