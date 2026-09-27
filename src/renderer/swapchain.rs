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
}

pub unsafe fn create_swapchain(
    device: &ash::Device,
    swapchain_loader: &ash::khr::swapchain::Device,
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    indices: QueueFamilyIndices,
    window_size: (u32, u32),
) -> Result<SwapchainData> {
    let support =
        unsafe { device::query_swapchain_support(surface_loader, physical_device, surface) }?;
    let caps = support.capabilities;

    let surface_format = choose_surface_format(&support.formats);
    let extent = choose_extent(&caps, window_size);

    // One more than the minimum so we're never stuck waiting on the driver.
    let mut image_count = caps.min_image_count + 1;
    if caps.max_image_count > 0 {
        image_count = image_count.min(caps.max_image_count);
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
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(sharing_mode)
        .queue_family_indices(&queue_families)
        .pre_transform(caps.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(choose_present_mode(&support.present_modes))
        .clipped(true);

    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }?;
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }?;
    let image_views = unsafe { create_image_views(device, &images, surface_format.format) }?;
    let render_finished = unsafe { sync::create_semaphores(device, images.len()) }?;

    Ok(SwapchainData {
        swapchain,
        images,
        image_views,
        render_finished,
        format: surface_format.format,
        extent,
    })
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
            swapchain_loader.destroy_swapchain(self.swapchain, None);
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

fn choose_extent(caps: &vk::SurfaceCapabilitiesKHR, (width, height): (u32, u32)) -> vk::Extent2D {
    // u32::MAX means the surface size is ours to pick within the allowed range.
    if caps.current_extent.width != u32::MAX {
        return caps.current_extent;
    }
    vk::Extent2D {
        width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
        height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
    }
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

    images
        .iter()
        .map(|&image| {
            let create_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(subresource_range);
            Ok(unsafe { device.create_image_view(&create_info, None) }?)
        })
        .collect()
}
