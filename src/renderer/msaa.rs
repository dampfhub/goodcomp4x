//! Multisample antialiasing: the scene is drawn into a multisampled color
//! target, which the render pass resolves (averages) into the swapchain image,
//! smoothing polygon edges.

use anyhow::{Context, Result};
use ash::vk;

use super::buffer;

const DEFAULT_SAMPLE_CAP: u32 = 8;

/// Read once at renderer startup. A single-sample target needs a different render
/// pass (no resolve attachment), so the testing override accepts 2 through 64.
pub fn sample_cap() -> u32 {
    let value = std::env::var("RENDER_MSAA").ok();
    match parse_sample_cap(value.as_deref()) {
        Some(cap) => cap,
        None => {
            log::warn!(
                "invalid RENDER_MSAA; expected 2, 4, 8, 16, 32 or 64; using {DEFAULT_SAMPLE_CAP}"
            );
            DEFAULT_SAMPLE_CAP
        }
    }
}

fn parse_sample_cap(value: Option<&str>) -> Option<u32> {
    match value {
        None => Some(DEFAULT_SAMPLE_CAP),
        Some(value) => value
            .parse::<u32>()
            .ok()
            .filter(|n| matches!(n, 2 | 4 | 8 | 16 | 32 | 64)),
    }
}

fn best_samples(supported: vk::SampleCountFlags, cap: u32) -> Result<vk::SampleCountFlags> {
    [64, 32, 16, 8, 4, 2]
        .into_iter()
        .filter(|&n| n <= cap)
        .map(vk::SampleCountFlags::from_raw)
        .find(|&count| supported.contains(count))
        .context("no supported multisample count within RENDER_MSAA cap")
}

/// Highest supported multisample count at or below the startup cap.
pub unsafe fn pick_samples(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    format: vk::Format,
    cap: u32,
) -> Result<vk::SampleCountFlags> {
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
    }
    .context("querying MSAA target format support")?;
    best_samples(
        limits.framebuffer_color_sample_counts & image.sample_counts,
        cap,
    )
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
        log::info!(
            "MSAA target {}x{}, {} samples: {:.1} MiB allocation",
            extent.width,
            extent.height,
            samples.as_raw(),
            requirements.size as f64 / 1_048_576.0
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_choice_never_exceeds_the_cap_or_supported_set() {
        for bits in 0..128 {
            let supported = vk::SampleCountFlags::from_raw(bits);
            for cap in 0..=64 {
                if let Ok(chosen) = best_samples(supported, cap) {
                    assert!(chosen.as_raw() <= cap);
                    assert!(supported.contains(chosen));
                    assert!(chosen.as_raw() >= 2);
                    assert!(
                        ![2, 4, 8, 16, 32, 64]
                            .into_iter()
                            .any(|n| n > chosen.as_raw()
                                && n <= cap
                                && supported.contains(vk::SampleCountFlags::from_raw(n)))
                    );
                } else {
                    assert!(
                        ![2, 4, 8, 16, 32, 64]
                            .into_iter()
                            .any(|n| n <= cap
                                && supported.contains(vk::SampleCountFlags::from_raw(n)))
                    );
                }
            }
        }
    }
    #[test]
    fn sample_choice_prefers_the_highest_supported_multisample_count() {
        use vk::SampleCountFlags as S;
        assert_eq!(best_samples(S::TYPE_4, 16).unwrap(), S::TYPE_4);
        assert_eq!(best_samples(S::TYPE_4 | S::TYPE_8, 16).unwrap(), S::TYPE_8);
        assert_eq!(
            best_samples(S::TYPE_4 | S::TYPE_8 | S::TYPE_16, 16).unwrap(),
            S::TYPE_16
        );
        assert_eq!(best_samples(S::TYPE_4 | S::TYPE_64, 16).unwrap(), S::TYPE_4);
    }

    #[test]
    fn sample_cap_parses_without_changing_process_environment() {
        assert_eq!(parse_sample_cap(None), Some(8));
        for n in [2, 4, 8, 16, 32, 64] {
            assert_eq!(parse_sample_cap(Some(&n.to_string())), Some(n));
        }
        for invalid in ["", "1", "0", "3", "128", "-4", "auto"] {
            assert_eq!(parse_sample_cap(Some(invalid)), None);
        }
    }
}
