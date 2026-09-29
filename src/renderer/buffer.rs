use anyhow::{Context, Result};
use ash::vk;

/// Creates a buffer and binds it to freshly allocated memory with `properties`.
pub unsafe fn create_buffer(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory)> {
    let (buffer, memory, _) = unsafe {
        create_buffer_preferred(
            instance,
            device,
            physical_device,
            size,
            usage,
            properties,
            &[],
        )
    }?;
    Ok((buffer, memory))
}

/// Required flags are never relaxed; preferences are tried in order.
#[allow(clippy::too_many_arguments)]
pub unsafe fn create_buffer_preferred(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    required: vk::MemoryPropertyFlags,
    preferences: &[vk::MemoryPropertyFlags],
) -> Result<(vk::Buffer, vk::DeviceMemory, vk::MemoryPropertyFlags)> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_info, None) }?;
    let allocate = || -> Result<_> {
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let types = &memory_properties.memory_types[..memory_properties.memory_type_count as usize];
        let index = choose_memory_type(types, requirements.memory_type_bits, required, preferences)
            .context("no GPU memory type supports the requested properties")?;
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(index);
        let memory = unsafe { device.allocate_memory(&info, None) }?;
        if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe { device.free_memory(memory, None) };
            return Err(error.into());
        }
        Ok((memory, types[index as usize].property_flags))
    };
    match allocate() {
        Ok((memory, properties)) => Ok((buffer, memory, properties)),
        Err(error) => {
            unsafe { device.destroy_buffer(buffer, None) };
            Err(error)
        }
    }
}

fn choose_memory_type(
    types: &[vk::MemoryType],
    bits: u32,
    required: vk::MemoryPropertyFlags,
    preferences: &[vk::MemoryPropertyFlags],
) -> Option<u32> {
    preferences
        .iter()
        .copied()
        .chain([vk::MemoryPropertyFlags::empty()])
        .find_map(|preferred| {
            types.iter().enumerate().find_map(|(i, memory)| {
                (bits & (1 << i) != 0 && memory.property_flags.contains(required | preferred))
                    .then_some(i as u32)
            })
        })
}

/// First memory type allowed by `type_bits` that has all of `properties`.
pub unsafe fn find_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    properties: vk::MemoryPropertyFlags,
) -> Result<u32> {
    let memory = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    (0..memory.memory_type_count)
        .find(|&i| {
            type_bits & (1 << i) != 0
                && memory.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
        })
        .context("no GPU memory type supports the requested properties")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_preferences_respect_required_flags_and_allowed_types() {
        use vk::MemoryPropertyFlags as F;
        let host = F::HOST_VISIBLE | F::HOST_COHERENT;
        let types = [
            host,
            host | F::DEVICE_LOCAL,
            F::DEVICE_LOCAL,
            F::HOST_VISIBLE | F::HOST_CACHED,
        ]
        .map(|property_flags| vk::MemoryType {
            property_flags,
            heap_index: 0,
        });
        assert_eq!(
            choose_memory_type(&types, 15, host, &[F::DEVICE_LOCAL]),
            Some(1)
        );
        assert_eq!(
            choose_memory_type(&types, 13, host, &[F::DEVICE_LOCAL]),
            Some(0)
        );
        assert_eq!(
            choose_memory_type(&types, 4, host, &[F::DEVICE_LOCAL]),
            None
        );
        assert_eq!(
            choose_memory_type(&types, 15, F::HOST_VISIBLE, &[F::HOST_CACHED]),
            Some(3)
        );
    }
}
