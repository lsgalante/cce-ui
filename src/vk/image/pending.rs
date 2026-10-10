//! Draining the upload queue each frame (`draw::images` hands over what was queued since the last),
//! and freeing images.

use super::*;

impl ImageStage {
    /// Drain the global upload/free queue. Uploads are synchronous one-time
    /// submits (rare: images load once); frees wait for device idle.
    pub(crate) fn process_pending(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) {
        if !self.shared_uploads {
            return;
        }
        let pending: Vec<Pending> = take_pending();
        if pending.is_empty() {
            self.staging_idle = self.staging_idle.saturating_add(1);
            if self.staging_idle > STAGING_IDLE_FRAMES {
                if let Some(mut idle) = self
                    .staging
                    .take_if(|b| b.size > STAGING_KEEP_BYTES)
                {
                    // Safe without a wait for the same reason `staging_for`
                    // needs none: every copy waits for itself before
                    // returning, so nothing is reading this buffer here.
                    destroy_cpu_buffer(device, allocator, &mut idle);
                }
            }
            return;
        }
        self.staging_idle = 0;
        let mut items = pending.into_iter().peekable();
        while let Some(item) = items.next() {
            match item {
                Pending::Upload { id, pixels, width, height, format, mips } => {
                    // The uploads queued back to back go up together: one
                    // command buffer, one submit, one wait (`upload_run`).
                    let mut bytes = pixels.len();
                    let mut owned = vec![(id, pixels, width, height, format, mips)];
                    while let Some(Pending::Upload { pixels, .. }) = items.peek() {
                        if bytes + pixels.len() > RUN_STAGING_BYTES {
                            break;
                        }
                        bytes += pixels.len();
                        let Some(Pending::Upload { id, pixels, width, height, format, mips }) = items.next() else {
                            unreachable!()
                        };
                        owned.push((id, pixels, width, height, format, mips));
                    }
                    let run: Vec<RunItem<'_>> = owned
                        .iter()
                        .map(|(id, pixels, width, height, format, mips)| RunItem {
                            id: *id,
                            pixels,
                            width: *width,
                            height: *height,
                            format: *format,
                            mips: *mips,
                        })
                        .collect();
                    self.upload_run(device, allocator, queue, command_pool, &run);
                    drop(run);
                    for (_, pixels, ..) in owned {
                        retire_buffer(pixels);
                    }
                }
                Pending::Update { id, pixels, width, height, format } => {
                    // Same picture, new contents: copy into the image that is
                    // already there. Anything else about it changing (a window
                    // resize) falls back to building a fresh one under the
                    // same id.
                    let reusable = self.images.get(&id).is_some_and(|gpu| {
                        gpu.width == width && gpu.height == height && gpu.format == format
                    });
                    if reusable {
                        self.write_into(device, allocator, queue, command_pool, id, &pixels);
                    } else {
                        // Mipmapped if what it replaces was: the id is the
                        // same picture at another size.
                        let mips = self.images.get(&id).is_some_and(|gpu| gpu.mip_levels > 1);
                        self.destroy_image(device, allocator, id);
                        self.upload(
                            device, allocator, queue, command_pool, id, &pixels, width, height,
                            format, mips,
                        );
                    }
                    retire_buffer(pixels);
                }
                Pending::UpdateRegions { id, pixels, width, height, format, regions } => {
                    // Only into the picture these regions were cut from. A
                    // fresh image here would be blank outside them, so a
                    // mismatch writes nothing; see `update_pixel_regions`.
                    let matches = self.images.get(&id).is_some_and(|gpu| {
                        gpu.width == width && gpu.height == height && gpu.format == format
                    });
                    if matches {
                        self.write_regions(
                            device, allocator, queue, command_pool, id, &pixels, &regions,
                        );
                    } else {
                        log::debug!("image {id}: region update for a {width}x{height} image it no longer matches, dropped");
                    }
                    retire_buffer(pixels);
                }
                Pending::Free { id } => {
                    // Frees queued together share one idle wait.
                    let mut ids = vec![id];
                    while let Some(Pending::Free { .. }) = items.peek() {
                        let Some(Pending::Free { id }) = items.next() else { unreachable!() };
                        ids.push(id);
                    }
                    self.destroy_images(device, allocator, &ids);
                }
            }
        }
    }

    /// Tear one image down. Destroying something the GPU may still be reading
    /// needs the device idle, which is why this is not on the per-frame path
    /// any more: a streaming caller updates in place and never gets here until
    /// it is finished with the image for good.
    pub(super) fn destroy_image(&mut self, device: &ash::Device, allocator: &mut Allocator, id: u32) {
        self.destroy_images(device, allocator, &[id]);
    }

    /// Tear several images down behind ONE idle wait. Until 2026-10-06 each
    /// free waited on its own, so leaving a folder of thumbnails ran a
    /// device-idle per thumbnail.
    pub(super) fn destroy_images(&mut self, device: &ash::Device, allocator: &mut Allocator, ids: &[u32]) {
        let gone: Vec<GpuImage> = ids.iter().filter_map(|id| self.images.remove(id)).collect();
        if gone.is_empty() {
            return;
        }
        unsafe {
            let _ = device.device_wait_idle();
        }
        for mut gpu in gone {
            unsafe {
                device.destroy_image_view(gpu.view, None);
                device.destroy_image(gpu.image, None);
                let _ = device.free_descriptor_sets(self.descriptor_pool, &[gpu.descriptor_set]);
            }
            if let Some(a) = gpu.allocation.take() {
                let _ = allocator.free(a);
            }
        }
    }
}
