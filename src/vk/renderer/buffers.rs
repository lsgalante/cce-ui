//! Host-visible buffers: create, destroy, and the null buffer.

use super::*;

pub(crate) struct AllocatedBuffer {
    pub(crate) buffer: vk::Buffer,
    pub(crate) allocation: Option<Allocation>,
    pub(crate) size: vk::DeviceSize,
}

impl AllocatedBuffer {
    pub(crate) fn null() -> Self {
        AllocatedBuffer { buffer: vk::Buffer::null(), allocation: None, size: 0 }
    }
}

/// Create a host-visible buffer bound to gpu-allocator memory.
pub(crate) fn create_cpu_buffer(
    device: &ash::Device,
    allocator: &mut Allocator,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    name: &str,
) -> AllocatedBuffer {
    unsafe {
        let buffer = device
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
            .expect("Failed to create buffer");
        let requirements = device.get_buffer_memory_requirements(buffer);
        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate buffer memory");
        device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .expect("Failed to bind buffer memory");
        AllocatedBuffer { buffer, allocation: Some(allocation), size }
    }
}

/// Destroy a buffer and return its memory to the allocator.
pub(crate) fn destroy_cpu_buffer(
    device: &ash::Device,
    allocator: &mut Allocator,
    buf: &mut AllocatedBuffer,
) {
    unsafe {
        self::destroy_buffer_handle(device, buf.buffer);
    }
    if let Some(allocation) = buf.allocation.take() {
        let _ = allocator.free(allocation);
    }
    buf.buffer = vk::Buffer::null();
    buf.size = 0;
}

pub(super) unsafe fn destroy_buffer_handle(device: &ash::Device, buffer: vk::Buffer) {
    if buffer != vk::Buffer::null() {
        device.destroy_buffer(buffer, None);
    }
}
