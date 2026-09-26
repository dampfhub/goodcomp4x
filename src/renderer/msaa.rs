//! Multisample antialiasing: the scene is drawn into a multisampled color
//! target, which the render pass resolves (averages) into the swapchain image,
//! smoothing polygon edges.

use anyhow::Result;
use ash::vk;

use super::buffer;

/// Samples per pixel. Vulkan requires every device to support 4 for color
/// attachments, so there's no need to query or fall back.
pub const SAMPLES: vk::SampleCountFlags = vk::SampleCountFlags::TYPE_4;

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
            .samples(SAMPLES)
            .tiling(vk::ImageTiling::OPTIMAL)
            // Only ever resolved, never read back, so it can live in
            // lazily allocated (tile) memory where the GPU offers it.
            .usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT,
            )
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

    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}
