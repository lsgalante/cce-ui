//! Meshes: a vertex buffer per `MeshId` / `LitMeshId`, replaced without waiting for the device (the
//! old buffer kept as a spare, tagged with the frames that may still read it), and the spares
//! reclaimed once those frames have finished.

use super::*;

pub(super) struct Mesh {
    pub(super) buffer: AllocatedBuffer,
    pub(super) count: u32,
    /// Buffers this mesh held before, each tagged with the frames that may
    /// still read it: every frame submitted before the tag. An update takes
    /// one no frame still reads in place of waiting for the device to go
    /// idle ([`Mesh::replace`]).
    pub(super) spare: Vec<(AllocatedBuffer, u64)>,
}

impl Mesh {
    pub(super) fn new(buffer: AllocatedBuffer, count: u32) -> Self {
        Mesh { buffer, count, spare: Vec::new() }
    }

    /// Put `bytes` (`count` vertices) in place of what the mesh holds,
    /// without waiting for the GPU. The frames already submitted may still
    /// read the mesh's buffer, so the bytes go into another — a spare no
    /// submitted frame still reads (`tag <= complete`) and big enough, or a
    /// new one — and the buffer they replace becomes a spare tagged `tag`,
    /// the frames submitted so far. Until 2026-10-07 an update waited for
    /// the device to go idle, reasoning that geometry updates are rare; a
    /// playing simulation updates its meshes every frame, and the wait put
    /// the CPU's work and the GPU's end to end, so a frame took both.
    pub(super) fn replace(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        bytes: &[u8],
        count: u32,
        (tag, complete): (u64, u64),
        label: &'static str,
    ) {
        let needed = (bytes.len() as vk::DeviceSize).max(64);
        let free = self.spare.iter().position(|(b, t)| *t <= complete && b.size >= needed);
        let fresh = match free {
            Some(i) => self.spare.swap_remove(i).0,
            None => create_cpu_buffer(device, allocator, needed.next_power_of_two(), vk::BufferUsageFlags::VERTEX_BUFFER, label),
        };
        let old = std::mem::replace(&mut self.buffer, fresh);
        self.spare.push((old, tag));
        if !bytes.is_empty() {
            self.buffer.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
        }
        self.count = count;
    }

    /// Release the spares no frame still reads that are too small for the
    /// mesh as it stands, and keep two of the rest: a steady playback reuses
    /// them and allocates nothing.
    pub(super) fn reclaim(&mut self, device: &ash::Device, allocator: &mut Allocator, complete: u64) {
        let current = self.buffer.size;
        let mut kept = 0;
        let mut i = 0;
        while i < self.spare.len() {
            let (size, tag) = (self.spare[i].0.size, self.spare[i].1);
            let free = tag <= complete;
            if free && (size < current || kept >= 2) {
                let (mut buffer, _) = self.spare.swap_remove(i);
                destroy_cpu_buffer(device, allocator, &mut buffer);
                continue;
            }
            kept += free as usize;
            i += 1;
        }
    }

    pub(super) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        let mut buffer = std::mem::replace(&mut self.buffer, AllocatedBuffer::null());
        destroy_cpu_buffer(device, allocator, &mut buffer);
        for (mut spare, _) in self.spare.drain(..) {
            destroy_cpu_buffer(device, allocator, &mut spare);
        }
    }
}

impl SceneStage {
    pub(crate) fn create_mesh(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        verts: &[Vertex3D],
    ) -> MeshId {
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let mut buffer = create_cpu_buffer(
            device,
            allocator,
            (bytes.len() as vk::DeviceSize).max(64),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "mesh",
        );
        if !bytes.is_empty() {
            buffer.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                .copy_from_slice(bytes);
        }
        self.meshes.push(Mesh::new(buffer, verts.len() as u32));
        MeshId(self.meshes.len() - 1)
    }

    /// Replace a mesh's vertices without waiting for the GPU: the frames in
    /// flight keep the buffer they read ([`Mesh::replace`]).
    pub(crate) fn update_mesh(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        id: MeshId,
        verts: &[Vertex3D],
    ) {
        let frames = (self.submitted, self.complete_before);
        self.meshes[id.0].replace(device, allocator, bytemuck::cast_slice(verts), verts.len() as u32, frames, "mesh");
    }

    /// After the renderer has waited on the fence of the slot the next frame
    /// will use: the frame that slot last carried has finished, and every
    /// frame before it, so the spares those frames read may be reused.
    pub(crate) fn frame_waited(&mut self, device: &ash::Device, allocator: &mut Allocator, frames_in_flight: u64) {
        self.complete_before = (self.submitted + 1).saturating_sub(frames_in_flight);
        let complete = self.complete_before;
        for mesh in self.meshes.iter_mut().chain(self.lit_meshes.iter_mut()) {
            mesh.reclaim(device, allocator, complete);
        }
    }

    pub(crate) fn create_lit_mesh(&mut self, device: &ash::Device, allocator: &mut Allocator, verts: &[LitVertex]) -> LitMeshId {
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let mut buffer = create_cpu_buffer(
            device,
            allocator,
            (bytes.len() as vk::DeviceSize).max(64),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "lit-mesh",
        );
        if !bytes.is_empty() {
            buffer.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
        }
        self.lit_meshes.push(Mesh::new(buffer, verts.len() as u32));
        LitMeshId(self.lit_meshes.len() - 1)
    }

    /// Replace a lit mesh's vertices without waiting for the GPU, as
    /// `update_mesh` does.
    pub(crate) fn update_lit_mesh(&mut self, device: &ash::Device, allocator: &mut Allocator, id: LitMeshId, verts: &[LitVertex]) {
        let frames = (self.submitted, self.complete_before);
        self.lit_meshes[id.0].replace(device, allocator, bytemuck::cast_slice(verts), verts.len() as u32, frames, "lit-mesh");
    }
}
