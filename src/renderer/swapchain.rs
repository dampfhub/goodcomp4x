use anyhow::Result;
use ash::vk;

use super::device::{self, QueueFamilyIndices};
use super::sync;

pub struct SwapchainData {
    pub swapchain: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    /// One per image: signaled when rendering to that image finishes, and
    /// waited on by its present. Per image rather than per frame in flight
    /// because no fence covers a present's wait: the only sign it is done is
    /// the same image being acquired again, so only then is its semaphore
    /// safe to signal again. Destroyed after the swapchain, which releases
    /// any it still holds.
    pub render_finished: Vec<vk::Semaphore>,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    /// Whether the images can be copied from (`TRANSFER_SRC`), for readback.
    pub readable: bool,
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn create_swapchain(
    device: &ash::Device,
    swapchain_loader: &ash::khr::swapchain::Device,
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    indices: QueueFamilyIndices,
    window_size: (u32, u32),
    old_swapchain: vk::SwapchainKHR,
) -> Result<Option<SwapchainData>> {
    let support =
        unsafe { device::query_swapchain_support(surface_loader, physical_device, surface) }?;
    let caps = support.capabilities;

    let surface_format = choose_surface_format(&support.formats);
    let Some(extent) = choose_extent(&caps, window_size) else {
        return Ok(None);
    };

    // One more than the minimum so we're never stuck waiting on the driver.
    let mut image_count = caps.min_image_count + 1;
    if caps.max_image_count > 0 {
        image_count = image_count.min(caps.max_image_count);
    }

    // Copyable too, where the surface allows it, so a frame can be read back
    // (`readback.rs`).
    let readable = caps
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_SRC);
    let mut usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
    if readable {
        usage |= vk::ImageUsageFlags::TRANSFER_SRC;
    }

    let queue_families = indices.unique_families();
    let sharing_mode = if queue_families.len() > 1 {
        vk::SharingMode::CONCURRENT
    } else {
        vk::SharingMode::EXCLUSIVE
    };

    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(usage)
        .image_sharing_mode(sharing_mode)
        .queue_family_indices(&queue_families)
        .pre_transform(caps.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(choose_present_mode(&support.present_modes))
        .clipped(true)
        .old_swapchain(old_swapchain);

    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }?;
    let images = match unsafe { swapchain_loader.get_swapchain_images(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe { swapchain_loader.destroy_swapchain(swapchain, None) };
            return Err(err.into());
        }
    };
    let image_views = match unsafe { create_image_views(device, &images, surface_format.format) } {
        Ok(views) => views,
        Err(err) => {
            unsafe { swapchain_loader.destroy_swapchain(swapchain, None) };
            return Err(err);
        }
    };
    let render_finished = match unsafe { sync::create_semaphores(device, images.len()) } {
        Ok(semaphores) => semaphores,
        Err(err) => {
            unsafe {
                for view in image_views {
                    device.destroy_image_view(view, None);
                }
                swapchain_loader.destroy_swapchain(swapchain, None);
            }
            return Err(err);
        }
    };

    Ok(Some(SwapchainData {
        swapchain,
        images,
        image_views,
        render_finished,
        format: surface_format.format,
        extent,
        readable,
    }))
}

impl SwapchainData {
    pub unsafe fn destroy(
        &mut self,
        device: &ash::Device,
        swapchain_loader: &ash::khr::swapchain::Device,
    ) {
        unsafe {
            for view in self.image_views.drain(..) {
                device.destroy_image_view(view, None);
            }
            let swapchain = std::mem::replace(&mut self.swapchain, vk::SwapchainKHR::null());
            if swapchain != vk::SwapchainKHR::null() {
                swapchain_loader.destroy_swapchain(swapchain, None);
            }
            self.images.clear();
            for semaphore in self.render_finished.drain(..) {
                device.destroy_semaphore(semaphore, None);
            }
        }
    }
}

fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    formats
        .iter()
        .copied()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_SRGB
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .unwrap_or(formats[0])
}

fn choose_present_mode(modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    if modes.contains(&vk::PresentModeKHR::MAILBOX) {
        vk::PresentModeKHR::MAILBOX
    } else {
        // FIFO is the only mode every driver must support.
        vk::PresentModeKHR::FIFO
    }
}

fn choose_extent(
    caps: &vk::SurfaceCapabilitiesKHR,
    (width, height): (u32, u32),
) -> Option<vk::Extent2D> {
    // A minimized surface may report a zero current extent even while winit
    // still has a nonzero cached window size. Wait for a drawable surface.
    let extent = if caps.current_extent.width != u32::MAX {
        caps.current_extent
    } else {
        if width == 0 || height == 0 {
            return None;
        }
        vk::Extent2D {
            width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    };
    (extent.width > 0 && extent.height > 0).then_some(extent)
}

unsafe fn create_image_views(
    device: &ash::Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>> {
    let subresource_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);

    let mut views = Vec::with_capacity(images.len());
    for &image in images {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(subresource_range);
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                return Err(err.into());
            }
        }
    }
    Ok(views)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_current_extent_has_no_swapchain_extent() {
        let caps = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: 0,
                height: 0,
            },
            ..Default::default()
        };
        assert!(choose_extent(&caps, (1280, 720)).is_none());
        let caps = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            },
            min_image_extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
            max_image_extent: vk::Extent2D {
                width: 4096,
                height: 4096,
            },
            ..Default::default()
        };
        assert!(choose_extent(&caps, (0, 720)).is_none());
        assert_eq!(choose_extent(&caps, (1280, 720)).unwrap().width, 1280);
    }
}
