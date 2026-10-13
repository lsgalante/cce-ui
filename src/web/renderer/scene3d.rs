//! The 3D half of the WebGPU renderer, with the `3d` feature: the scene pass (`web::scene`) and
//! the path tracer (`web::rt`) into the backdrop, and the `Stage3D` impl apps stage them through.

use super::*;
use crate::draw::rt::{PreparedRtScene, RtCamera};
use crate::draw::scene::{MeshId, SceneDraw, SceneImage, Stage3D, Vertex3D};

impl WebRenderer {
    /// The scene's backdrop follows the canvas; a resized one holds no
    /// scene until the next is drawn into it, as on Vulkan.
    pub(super) fn fit_scene(&mut self, w: u32, h: u32) -> Result<(), JsValue> {
        let rt_staged = self.rt.as_ref().is_some_and(|rt| rt.staged());
        if self.scene.has_staged() || rt_staged || self.scene.target.is_some() {
            let had = self.scene.target.as_ref().map(|t| (t.width, t.height));
            self.scene.fit(&self.device, w, h)?;
            if had != Some((w, h)) {
                let t = self.scene.target.as_ref().expect("fit made one");
                self.scene_group_0 = Some(Self::group_0(
                    &self.device,
                    &self.layout_0,
                    &t.view,
                    &self.backdrop_sampler,
                    &self.window_info,
                    &self.plate_features,
                ));
            }
        }
        Ok(())
    }

    /// The 3D pass and the traced pane, into the backdrop, recorded first on `encoder`. With a
    /// scene shown, the backdrop is copied into `target` and the `@group(0)` over it is
    /// returned, for the UI pass to load (rather than clear) and sample for its blur plates.
    pub(super) fn record_scene(
        &mut self,
        encoder: &GpuCommandEncoder,
        target: &GpuTexture,
        w: u32,
        h: u32,
    ) -> Result<Option<GpuBindGroup>, JsValue> {
        let images_for_scene = &self.images;
        self.scene.record(&self.device, &self.queue, encoder, &|id| images_for_scene.get(&id).map(|i| i.group.clone()))?;
        // The traced pane, into the same backdrop after the raster scene.
        if let (Some(rt), Some(t)) = (self.rt.as_mut(), self.scene.target.as_ref()) {
            let image_view = |id: u32| {
                let img = images_for_scene.get(&id)?;
                Some((img.texture.create_view().ok()?, img.width, img.height))
            };
            if rt.record(&self.device, &self.queue, encoder, &t.view, (t.width, t.height), &image_view)? {
                self.scene.backdrop_valid = true;
            }
        }
        Ok(match (&self.scene_group_0, &self.scene.target) {
            (Some(group), Some(t)) if self.scene.backdrop_valid => {
                encoder.copy_texture_to_texture_with_gpu_extent_3d_dict(
                    &GpuTexelCopyTextureInfo::new(&t.backdrop),
                    &GpuTexelCopyTextureInfo::new(target),
                    &extent(w, h),
                )?;
                Some(group.clone())
            }
            _ => None,
        })
    }
}

/// The 3D half of the renderer (see `draw::scene::Stage3D`): the scene pass
/// in `web::scene`.
impl Stage3D for WebRenderer {
    fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId {
        self.scene.create_mesh(&self.device, &self.queue, verts)
    }
    fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]) {
        self.scene.update_mesh(&self.device, &self.queue, id, verts)
    }
    fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.scene.stage(scissor, draws)
    }
    fn stage_scene_images(&mut self, images: Vec<SceneImage>) {
        self.scene.stage_images(images)
    }
    fn set_scene_light(&mut self, toward: [f32; 3]) {
        let v = glam::Vec3::from_array(toward);
        if v.length_squared() > 1e-12 {
            self.scene.light = v.normalize().to_array();
        }
    }
    fn set_rt_scene_prepared(&mut self, scene: &PreparedRtScene) {
        if self.rt.is_none() {
            match WebRt::new(&self.device, self.view_format) {
                Ok(rt) => self.rt = Some(rt),
                Err(e) => {
                    web_sys::console::error_2(&"cce-ui: no path tracer:".into(), &e);
                    return;
                }
            }
        }
        let rt = self.rt.as_mut().unwrap();
        if let Err(e) = rt.set_scene(&self.device, &self.queue, scene) {
            web_sys::console::error_2(&"cce-ui: the traced scene was not uploaded:".into(), &e);
        }
    }
    fn set_rt_environment(&mut self, environment: RtEnvironment) {
        self.rt_environment = environment;
    }
    fn set_rt_background(&mut self, color: Option<[f32; 3]>) {
        self.rt_background = color;
    }
    fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        if let Some(rt) = self.rt.as_mut() {
            rt.set_background(self.rt_background);
            rt.set_environment(self.rt_environment);
            if let Err(e) = rt.stage(&self.device, pane, camera) {
                web_sys::console::error_2(&"cce-ui: the traced pane was not staged:".into(), &e);
            }
        }
    }
    fn rt_accumulating(&self) -> bool {
        self.rt.as_ref().is_some_and(|rt| rt.accumulating())
    }
}
