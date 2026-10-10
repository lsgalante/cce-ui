//! What a window's `VkSurfaceKHR` is made from (`SurfaceTarget`: a Wayland surface, or a
//! `CAMetalLayer`), and `SurfaceLost`, the error a dead display connection surfaces as.

use super::*;

/// What a window's `VkSurfaceKHR` is made from: the window system's own
/// handles, as raw pointers.
#[derive(Debug, Clone, Copy)]
pub enum SurfaceTarget {
    /// A `wl_display` and a `wl_surface` on it.
    Wayland { display: *mut c_void, surface: *mut c_void },
    /// A `CAMetalLayer` (macOS, through MoltenVK's VK_EXT_metal_surface).
    Metal { layer: *const c_void },
}

impl SurfaceTarget {
    /// Make the surface on `instance`, or fail as [`SurfaceLost`] (a dead
    /// display connection). A loader with no extension for this kind of
    /// window is a broken install, and panics naming it.
    pub(super) unsafe fn create(self, instance: &ash::Instance) -> Result<vk::SurfaceKHR, SurfaceLost> {
        let shared = shared_instance();
        match self {
            SurfaceTarget::Wayland { display, surface } => {
                if !shared.has_wayland_surface {
                    panic!("Vulkan loader offers no VK_KHR_wayland_surface but a window was requested");
                }
                ash::khr::wayland_surface::Instance::new(&shared.entry, instance)
                    .create_wayland_surface(
                        &vk::WaylandSurfaceCreateInfoKHR::default().display(display).surface(surface),
                        None,
                    )
                    .map_err(|result| SurfaceLost { call: "vkCreateWaylandSurfaceKHR", result })
            }
            SurfaceTarget::Metal { layer } => {
                if !shared.has_metal_surface {
                    panic!("Vulkan loader offers no VK_EXT_metal_surface (is MoltenVK installed?) but a window was requested");
                }
                ash::ext::metal_surface::Instance::new(&shared.entry, instance)
                    .create_metal_surface(&vk::MetalSurfaceCreateInfoEXT::default().layer(layer.cast()), None)
                    .map_err(|result| SurfaceLost { call: "vkCreateMetalSurfaceEXT", result })
            }
        }
    }
}

/// A Vulkan call on a window surface failed — in practice
/// `ERROR_SURFACE_LOST_KHR`: the display connection under the surface is dead,
/// because the compositor exited (a logout) or the transport broke. Mesa's
/// Wayland WSI answers the surface queries with a roundtrip, so they are the
/// first thing to find out.
///
/// That is the client's SESSION ending, not a renderer bug, so the window
/// constructors and the swapchain path report it instead of panicking, and the
/// caller ends the session the way it would for any other lost connection.
/// Until 2026-09-25 each of these calls `expect`ed, and a daemon asked for a
/// window over a dead connection took the whole process down at logout
/// (cce-cloud, `No surface formats: ERROR_SURFACE_LOST_KHR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceLost {
    /// The Vulkan entry point that failed.
    pub call: &'static str,
    pub result: vk::Result,
}

impl std::fmt::Display for SurfaceLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "window surface lost ({}: {})", self.call, self.result)
    }
}

impl std::error::Error for SurfaceLost {}
