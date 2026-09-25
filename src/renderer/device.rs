use std::ffi::CStr;

use anyhow::{Context, Result};
use ash::vk;

const DEVICE_EXTENSIONS: [&CStr; 1] = [ash::khr::swapchain::NAME];

#[derive(Clone, Copy)]
pub struct QueueFamilyIndices {
    pub graphics: u32,
    pub present: u32,
}

impl QueueFamilyIndices {
    pub fn unique_families(&self) -> Vec<u32> {
        if self.graphics == self.present {
            vec![self.graphics]
        } else {
            vec![self.graphics, self.present]
        }
    }
}

pub struct SwapchainSupport {
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub formats: Vec<vk::SurfaceFormatKHR>,
    pub present_modes: Vec<vk::PresentModeKHR>,
}

pub unsafe fn query_swapchain_support(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Result<SwapchainSupport> {
    unsafe {
        Ok(SwapchainSupport {
            capabilities: surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface)?,
            formats: surface_loader
                .get_physical_device_surface_formats(physical_device, surface)?,
            present_modes: surface_loader
                .get_physical_device_surface_present_modes(physical_device, surface)?,
        })
    }
}

/// Picks the best GPU that can render and present to `surface`, preferring
/// discrete over integrated.
pub unsafe fn pick_physical_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, QueueFamilyIndices)> {
    let mut best: Option<(vk::PhysicalDevice, QueueFamilyIndices, u32)> = None;

    for physical_device in unsafe { instance.enumerate_physical_devices() }? {
        let Some(indices) =
            (unsafe { find_queue_families(instance, surface_loader, physical_device, surface) })?
        else {
            continue;
        };
        if !(unsafe { supports_extensions(instance, physical_device) })? {
            continue;
        }
        let support = unsafe { query_swapchain_support(surface_loader, physical_device, surface) }?;
        if support.formats.is_empty() || support.present_modes.is_empty() {
            continue;
        }

        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let score = match properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 2,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
            _ => 0,
        };
        if best.is_none_or(|(_, _, best_score)| score > best_score) {
            best = Some((physical_device, indices, score));
        }
    }

    let (physical_device, indices, _) =
        best.context("no GPU supports rendering and presenting to this window")?;

    let properties = unsafe { instance.get_physical_device_properties(physical_device) };
    let name = properties
        .device_name_as_c_str()
        .unwrap_or(c"<unknown>")
        .to_string_lossy();
    log::info!("selected physical device: {name}");

    Ok((physical_device, indices))
}

pub unsafe fn create_logical_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    indices: QueueFamilyIndices,
) -> Result<(ash::Device, vk::Queue, vk::Queue)> {
    let priorities = [1.0];
    let queue_create_infos: Vec<_> = indices
        .unique_families()
        .into_iter()
        .map(|family| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)
        })
        .collect();

    let extension_names = DEVICE_EXTENSIONS.map(CStr::as_ptr);
    let features = vk::PhysicalDeviceFeatures::default();
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_names)
        .enabled_features(&features);

    let device = unsafe { instance.create_device(physical_device, &create_info, None) }?;
    let graphics_queue = unsafe { device.get_device_queue(indices.graphics, 0) };
    let present_queue = unsafe { device.get_device_queue(indices.present, 0) };

    Ok((device, graphics_queue, present_queue))
}

unsafe fn find_queue_families(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Result<Option<QueueFamilyIndices>> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    let mut graphics = None;
    let mut present = None;
    for (index, family) in (0..).zip(&families) {
        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics = Some(index);
        }
        if unsafe {
            surface_loader.get_physical_device_surface_support(physical_device, index, surface)
        }? {
            present = Some(index);
        }
        if let (Some(graphics), Some(present)) = (graphics, present) {
            return Ok(Some(QueueFamilyIndices { graphics, present }));
        }
    }
    Ok(None)
}

unsafe fn supports_extensions(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<bool> {
    let available = unsafe { instance.enumerate_device_extension_properties(physical_device) }?;
    Ok(DEVICE_EXTENSIONS.iter().all(|&required| {
        available
            .iter()
            .any(|ext| ext.extension_name_as_c_str() == Ok(required))
    }))
}
