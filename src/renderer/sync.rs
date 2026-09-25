use anyhow::Result;
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// Per-frame-in-flight synchronization primitives.
pub struct SyncObjects {
    pub image_available: Vec<vk::Semaphore>,
    pub render_finished: Vec<vk::Semaphore>,
    pub in_flight: Vec<vk::Fence>,
}

pub unsafe fn create_sync_objects(device: &ash::Device) -> Result<SyncObjects> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    // Start signaled so the first wait on each frame's fence doesn't block forever.
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let mut sync = SyncObjects {
        image_available: Vec::with_capacity(MAX_FRAMES_IN_FLIGHT),
        render_finished: Vec::with_capacity(MAX_FRAMES_IN_FLIGHT),
        in_flight: Vec::with_capacity(MAX_FRAMES_IN_FLIGHT),
    };
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        unsafe {
            sync.image_available
                .push(device.create_semaphore(&semaphore_info, None)?);
            sync.render_finished
                .push(device.create_semaphore(&semaphore_info, None)?);
            sync.in_flight.push(device.create_fence(&fence_info, None)?);
        }
    }
    Ok(sync)
}

impl SyncObjects {
    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            for &semaphore in self.image_available.iter().chain(&self.render_finished) {
                device.destroy_semaphore(semaphore, None);
            }
            for &fence in &self.in_flight {
                device.destroy_fence(fence, None);
            }
        }
    }
}
