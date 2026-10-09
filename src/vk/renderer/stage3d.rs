//! The 3D side: meshes, lit meshes, images in the scene, the path tracer's scene and camera, and
//! the `Stage3D` / `LitStage3D` impls that forward to them.

use super::*;

/// The 3D half of the renderer, as an app stages it through
/// `Application::init_3d` / `stage_3d` — each method is the inherent one.
impl crate::draw::scene::Stage3D for VkRenderer {
    fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId {
        VkRenderer::create_mesh(self, verts)
    }
    fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]) {
        VkRenderer::update_mesh(self, id, verts)
    }
    fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        VkRenderer::stage_scene(self, scissor, draws)
    }
    fn stage_scene_images(&mut self, images: Vec<SceneImage>) {
        VkRenderer::stage_scene_images(self, images)
    }
    fn set_scene_light(&mut self, toward: [f32; 3]) {
        VkRenderer::set_scene_light(self, toward)
    }
    fn set_rt_scene_with_image(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial], image: Option<RtImage>) {
        VkRenderer::set_rt_scene_with_image(self, triangles, materials, image)
    }
    fn set_rt_scene_prepared(&mut self, scene: &PreparedRtScene) {
        VkRenderer::set_rt_scene_prepared(self, scene)
    }
    fn rt_needs_bvh(&self) -> bool {
        VkRenderer::rt_needs_bvh(self)
    }
    fn set_rt_environment(&mut self, environment: RtEnvironment) {
        VkRenderer::set_rt_environment(self, environment)
    }
    fn set_rt_background(&mut self, color: Option<[f32; 3]>) {
        VkRenderer::set_rt_background(self, color)
    }
    fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        VkRenderer::stage_rt(self, pane, camera)
    }
    fn rt_accumulating(&self) -> bool {
        VkRenderer::rt_accumulating(self)
    }
    fn lit(&mut self) -> Option<&mut dyn crate::draw::lit::LitStage3D> {
        Some(self)
    }
}

impl crate::draw::lit::LitStage3D for VkRenderer {
    fn create_lit_mesh(&mut self, verts: &[crate::draw::lit::LitVertex]) -> crate::draw::lit::LitMeshId {
        VkRenderer::create_lit_mesh(self, verts)
    }
    fn update_lit_mesh(&mut self, id: crate::draw::lit::LitMeshId, verts: &[crate::draw::lit::LitVertex]) {
        VkRenderer::update_lit_mesh(self, id, verts)
    }
    fn set_lit_light(&mut self, light: crate::draw::lit::LitLight) {
        VkRenderer::set_lit_light(self, light)
    }
    fn stage_lit(&mut self, draws: Vec<crate::draw::lit::LitDraw>) {
        VkRenderer::stage_lit(self, draws)
    }
}

impl VkRenderer {

    /// Upload a 3D mesh (Vertex3D: position + color); the id is stable for the
    /// renderer's lifetime.
    pub fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId {
        self.scene
            .create_mesh(&self.core.device, self.core.allocator.as_mut().unwrap(), verts)
    }

    /// Replace a mesh's vertices, without waiting for the GPU: the frames
    /// in flight keep the buffer they read, and the new vertices go into
    /// another (`SceneStage::update_mesh`). It waited for the device to go
    /// idle until 2026-10-07, which every frame of a playing simulation
    /// paid.
    pub fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]) {
        self.scene
            .update_mesh(&self.core.device, self.core.allocator.as_mut().unwrap(), id, verts);
    }

    /// Stage the 3D scene for the next `draw_frame`. Draws render into the
    /// backdrop image (scissored to the viewport pane, physical pixels), which
    /// is copied beneath the UI and doubles as the blur-behind source. Frames
    /// with no staged scene reuse the previous backdrop — the ash equivalent of
    /// the app's viewport-changed cache.
    pub fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.want_scene_targets();
        self.scene.stage(scissor, draws);
    }

    /// Grow the backdrop and depth targets to the surface the first time a
    /// scene is staged; until then they are 1×1 (`Scene::target_extent`).
    /// Runs between frames, the device idle, as a resize does.
    pub(super) fn want_scene_targets(&mut self) {
        if self.scene.wanted {
            return;
        }
        self.scene.wanted = true;
        if self.surface == vk::SurfaceKHR::null() {
            return; // sized with the swapchain when one exists
        }
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        self.sync_backdrop_targets();
    }

    /// Add textured quads to the scene staged by the last [`stage_scene`] —
    /// user images (ids from `upload_rgba`) standing in the 3D world, depth
    /// tested against the meshes. Call it AFTER `stage_scene`, which starts
    /// every staged scene with none; with no scene staged it does nothing.
    ///
    /// [`stage_scene`]: Self::stage_scene
    pub fn stage_scene_images(&mut self, images: Vec<SceneImage>) {
        self.scene.stage_images(images);
    }

    /// Upload a lit mesh (`draw::lit`). The first one also uploads the 1x1
    /// white image an untextured lit draw binds.
    pub fn create_lit_mesh(&mut self, verts: &[crate::draw::lit::LitVertex]) -> crate::draw::lit::LitMeshId {
        if self.scene.lit_fallback_image.is_none() {
            self.scene.lit_fallback_image = Some(self.upload_rgba_now(&[255, 255, 255, 255], 1, 1));
        }
        self.scene.create_lit_mesh(&self.core.device, self.core.allocator.as_mut().unwrap(), verts)
    }

    /// Replace a lit mesh's vertices; waits for the GPU first, as `update_mesh`.
    pub fn update_lit_mesh(&mut self, id: crate::draw::lit::LitMeshId, verts: &[crate::draw::lit::LitVertex]) {
        // No wait for the device: as `update_mesh`.
        self.scene.update_lit_mesh(&self.core.device, self.core.allocator.as_mut().unwrap(), id, verts);
    }

    /// The light lit draws are shaded by.
    pub fn set_lit_light(&mut self, light: crate::draw::lit::LitLight) {
        self.scene.lit_light = light;
    }

    /// This frame's lit draws, after `stage_scene`.
    pub fn stage_lit(&mut self, draws: Vec<crate::draw::lit::LitDraw>) {
        self.scene.stage_lit(draws);
    }

    /// Replace the path tracer's scene (triangles in the space the camera's
    /// `inv_mvp` unprojects into). Builds the BVH on the CPU and uploads it;
    /// waits for the GPU to go idle first — scene replacement is rare
    /// (geometry rebuilds), matching `update_mesh`. The first call compiles
    /// the compute pipeline. A large scene freezes the caller for the BVH
    /// build: build a [`PreparedRtScene`] on a worker instead and hand it to
    /// [`set_rt_scene_prepared`](Self::set_rt_scene_prepared).
    pub fn set_rt_scene(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial]) {
        self.set_rt_scene_with_image(triangles, materials, None);
    }

    /// [`set_rt_scene`](Self::set_rt_scene), with a user image standing in
    /// the scene: the picture the raster pass draws as a `SceneImage`,
    /// traced. The image's pixels changing is a change of scene like any
    /// other — set it again, which restarts the accumulation.
    pub fn set_rt_scene_with_image(
        &mut self,
        triangles: &[RtTriangle],
        materials: &[RtMaterial],
        image: Option<RtImage>,
    ) {
        let prepared = PreparedRtScene::new(triangles.to_vec(), materials, image, self.rt_needs_bvh());
        self.set_rt_scene_prepared(&prepared);
    }

    /// Replace the path tracer's scene with one prepared off this thread
    /// ([`PreparedRtScene::new`]): only the upload — buffers written, and
    /// on the ray-query tier the acceleration structures built on the GPU.
    /// Waits for the GPU to go idle first, as `set_rt_scene` does. A scene
    /// prepared without a BVH gets one built here if this renderer needs it.
    pub fn set_rt_scene_prepared(&mut self, scene: &PreparedRtScene) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        let core = &mut self.core;
        let allocator = core.allocator.as_mut().unwrap();
        let rt = self.rt.get_or_insert_with(|| {
            RtStage::new(
                &core.device,
                allocator,
                FRAMES_IN_FLIGHT,
                core.accel_loader.as_ref(),
                core.as_scratch_align,
                core.min_uniform_align,
                core.queue,
                core.command_pool,
            )
        });
        rt.set_scene(
            &core.device,
            allocator,
            core.queue,
            core.command_pool,
            &scene.packed,
            scene.image.map(|i| (RtImageSource::Shared(i.image), i.corners, i.opacity)),
        );
    }

    /// Whether this renderer's tracer traverses a CPU-built BVH (the compute
    /// tier) rather than building driver acceleration structures (the
    /// hardware ray-query tier): the `with_bvh` a [`PreparedRtScene`] for it
    /// wants. Answered before the first scene from the device's features.
    pub fn rt_needs_bvh(&self) -> bool {
        match &self.rt {
            Some(rt) => rt.needs_bvh(),
            None => crate::vk::rt::needs_bvh(self.core.accel_loader.is_some()),
        }
    }

    /// The direction TOWARD the 3D pass's light, in world space (any
    /// length; zero is ignored). The flat shading reads it, and a host that
    /// bakes smooth shading (`SceneDraw::prelit`) should light by the same
    /// one. Until a host sets it the light is the one this pass always had.
    /// A traced pane has its own, in [`RtEnvironment`]; a host that wants
    /// the two views to agree hands both the same direction.
    pub fn set_scene_light(&mut self, toward: [f32; 3]) {
        let v = glam::Vec3::from_array(toward);
        if v.length_squared() > 1e-12 {
            self.scene.light = v.normalize().to_array();
        }
    }

    /// The traced pane's sky and sun — see [`RtEnvironment`]. Kept here and
    /// handed over at each `stage_rt`, like the background; a change
    /// restarts the accumulation.
    pub fn set_rt_environment(&mut self, environment: RtEnvironment) {
        self.rt_environment = environment;
    }

    /// What a camera ray that meets nothing shows in the traced pane: a
    /// colour in LINEAR RGB (what the raster pass's vertex colours are), or
    /// None for the sky, which is what every miss showed until 2026-10-02.
    /// Only the camera ray: a bounce that leaves the scene still meets the
    /// sky, the tracer's one light, so a backdrop changes what is seen
    /// behind the scene and not how it is lit. Kept here and handed over at
    /// each `stage_rt`, so it may be set before the first scene; a change
    /// restarts the accumulation.
    pub fn set_rt_background(&mut self, color: Option<[f32; 3]>) {
        self.rt_background = color;
    }

    /// Stage one progressive path-tracing pass into the viewport pane
    /// (physical pixels) for the next `draw_frame`. Call it every frame while
    /// RT mode is on: each frame adds a sample; a camera/pane/scene change
    /// restarts the accumulation. No-op until `set_rt_scene` has run.
    pub fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        // The tracer blits into the backdrop at the surface's size.
        self.want_scene_targets();
        if let Some(rt) = self.rt.as_mut() {
            rt.set_background(self.rt_background);
            rt.set_environment(self.rt_environment);
            rt.stage(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                pane,
                camera,
            );
        }
    }

    /// True while more `stage_rt` + `draw_frame` rounds would still refine the
    /// image — the app's cue to keep requesting frames.
    pub fn rt_accumulating(&self) -> bool {
        self.rt.as_ref().is_some_and(|rt| rt.accumulating())
    }
}
