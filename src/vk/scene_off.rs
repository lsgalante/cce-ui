//! The scene stage without the `3d` feature: only its backdrop. The 2D shader samples binding
//! 0 whether or not a scene was ever drawn (the frosted root plate reads it, zeroed), so the
//! renderer still needs an image there — a 1×1 one, as the 3D stage makes before its first
//! scene (`scene::SceneStage::target_extent`). Same names as that stage, so the renderer calls
//! one interface either way; nothing here ever records, so the backdrop never becomes valid.

use ash::vk;
use gpu_allocator::vulkan::{Allocation, Allocator};

use super::renderer::create_backdrop_image;

pub(crate) struct SceneStage {
    format: vk::Format,
    pub(crate) backdrop_image: vk::Image,
    pub(crate) backdrop_view: vk::ImageView,
    backdrop_allocation: Option<Allocation>,
    /// Always false: there is no scene to put in the backdrop.
    pub(crate) backdrop_valid: bool,
    /// Always false: nothing can stage a scene.
    pub(crate) wanted: bool,
    /// Frames submitted, counted by the renderer as on the 3D stage; unread here.
    pub(crate) submitted: u64,
}

impl SceneStage {
    pub(crate) fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        format: vk::Format,
        _extent: vk::Extent2D,
        _frames_in_flight: usize,
        _min_uniform_align: vk::DeviceSize,
        _max_line_width: f32,
    ) -> Self {
        let mut stage = SceneStage {
            format,
            backdrop_image: vk::Image::null(),
            backdrop_view: vk::ImageView::null(),
            backdrop_allocation: None,
            backdrop_valid: false,
            wanted: false,
            submitted: 0,
        };
        stage.resize(device, allocator, vk::Extent2D { width: 1, height: 1 });
        stage
    }

    pub(crate) fn target_extent(&self, _surface: vk::Extent2D) -> vk::Extent2D {
        vk::Extent2D { width: 1, height: 1 }
    }

    /// Makes the 1×1 backdrop the first time; it never changes size after.
    pub(crate) fn resize(&mut self, device: &ash::Device, allocator: &mut Allocator, extent: vk::Extent2D) -> bool {
        if self.backdrop_view != vk::ImageView::null() {
            return false;
        }
        let (image, view, allocation) = create_backdrop_image(device, allocator, self.format, extent);
        self.backdrop_image = image;
        self.backdrop_view = view;
        self.backdrop_allocation = Some(allocation);
        true
    }

    pub(crate) fn record(
        &mut self,
        _device: &ash::Device,
        _cmd: vk::CommandBuffer,
        _frame_index: usize,
        _images: &crate::vk::image::ImageStage,
    ) -> bool {
        false
    }

    pub(crate) fn frame_waited(&mut self, _device: &ash::Device, _allocator: &mut Allocator, _frames_in_flight: u64) {}

    pub(crate) fn write_frame_uniforms(
        &mut self,
        _device: &ash::Device,
        _allocator: &mut Allocator,
        _frame_index: usize,
        _corner_radius_px: f32,
    ) {
    }

    pub(crate) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            if self.backdrop_view != vk::ImageView::null() {
                device.destroy_image_view(self.backdrop_view, None);
                device.destroy_image(self.backdrop_image, None);
                self.backdrop_view = vk::ImageView::null();
                self.backdrop_image = vk::Image::null();
            }
        }
        if let Some(a) = self.backdrop_allocation.take() {
            let _ = allocator.free(a);
        }
    }
}
