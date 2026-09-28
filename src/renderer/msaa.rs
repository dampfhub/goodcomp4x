//! Multisample antialiasing: the scene is drawn into a multisampled color
//! target, which the render pass resolves (averages) into the swapchain image,
//! smoothing polygon edges.

use anyhow::Result;
use ash::vk;

use super::buffer;

/// Sample counts to try, most first. Vulkan requires every device to support
/// 4 for color attachments, so the search always succeeds by then.
const PREFERRED_SAMPLES: [vk::SampleCountFlags; 3] = [
    vk::SampleCountFlags::TYPE_16,
    vk::SampleCountFlags::TYPE_8,
    vk::SampleCountFlags::TYPE_4,
];

/// The most samples per pixel the device supports for a color target of
/// `format`: more samples mean smoother edges.
pub unsafe fn pick_samples(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    format: vk::Format,
) -> vk::SampleCountFlags {
    let limits = unsafe { instance.get_physical_device_properties(physical_device) }.limits;
    let image = unsafe {
        instance.get_physical_device_image_format_properties(
            physical_device,
            format,
            vk::ImageType::TYPE_2D,
            vk::ImageTiling::OPTIMAL,
            USAGE,
            vk::ImageCreateFlags::empty(),
        )
    };
    let supported = limits.framebuffer_color_sample_counts
        & image.map_or(vk::SampleCountFlags::TYPE_4, |p| p.sample_counts);
    PREFERRED_SAMPLES
        .into_iter()
        .find(|&count| supported.contains(count))
        .unwrap_or(vk::SampleCountFlags::TYPE_4)
}

/// Only ever resolved, never read back, so the target can live in lazily
/// allocated (tile) memory where the GPU offers it.
const USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
    vk::ImageUsageFlags::COLOR_ATTACHMENT.as_raw()
        | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT.as_raw(),
);

/// The multisampled image the scene is drawn into, sized to the swapchain.
pub struct ColorTarget {
    image: vk::Image,
    memory: vk::DeviceMemory,
    pub view: vk::ImageView,
}

impl ColorTarget {
    pub unsafe fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        format: vk::Format,
        extent: vk::Extent2D,
        samples: vk::SampleCountFlags,
    ) -> Result<Self> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(samples)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(USAGE)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }?;

        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let find = |properties| unsafe {
            buffer::find_memory_type(
                instance,
                physical_device,
                requirements.memory_type_bits,
                properties,
            )
        };
        let memory_type =
            find(vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::LAZILY_ALLOCATED)
                .or_else(|_| find(vk::MemoryPropertyFlags::DEVICE_LOCAL))?;
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = unsafe { device.allocate_memory(&alloc_info, None) }?;
        unsafe { device.bind_image_memory(image, memory, 0) }?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );
        let view = unsafe { device.create_image_view(&view_info, None) }?;

        Ok(Self {
            image,
            memory,
            view,
        })
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            let view = std::mem::replace(&mut self.view, vk::ImageView::null());
            if view != vk::ImageView::null() {
                device.destroy_image_view(view, None);
            }
            let image = std::mem::replace(&mut self.image, vk::Image::null());
            if image != vk::Image::null() {
                device.destroy_image(image, None);
            }
            let memory = std::mem::replace(&mut self.memory, vk::DeviceMemory::null());
            if memory != vk::DeviceMemory::null() {
                device.free_memory(memory, None);
            }
        }
    }
}
