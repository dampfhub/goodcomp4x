use anyhow::Result;
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// Per-frame-in-flight synchronization primitives. The semaphores that
/// presentation waits on are per swapchain image instead, and live in
/// `SwapchainData::render_finished` (see `create_semaphores`).
pub struct SyncObjects {
    pub image_available: Vec<vk::Semaphore>,
    pub in_flight: Vec<vk::Fence>,
}

pub unsafe fn create_sync_objects(device: &ash::Device) -> Result<SyncObjects> {
    // Start signaled so the first wait on each frame's fence doesn't block forever.
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let image_available = unsafe { create_semaphores(device, MAX_FRAMES_IN_FLIGHT) }?;
    let in_flight = (0..MAX_FRAMES_IN_FLIGHT)
        .map(|_| Ok(unsafe { device.create_fence(&fence_info, None) }?))
        .collect::<Result<Vec<_>>>()?;
    Ok(SyncObjects {
        image_available,
        in_flight,
    })
}

/// `count` new binary semaphores.
pub unsafe fn create_semaphores(device: &ash::Device, count: usize) -> Result<Vec<vk::Semaphore>> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    (0..count)
        .map(|_| Ok(unsafe { device.create_semaphore(&semaphore_info, None) }?))
        .collect()
}

impl SyncObjects {
    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            for &semaphore in &self.image_available {
                device.destroy_semaphore(semaphore, None);
            }
            for &fence in &self.in_flight {
                device.destroy_fence(fence, None);
            }
        }
    }
}
