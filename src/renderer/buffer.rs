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

/// Required flags are never relaxed; preferences are tried in order. When a
/// memory type is out of room (a 256 MiB device-local BAR heap, say), the
/// next type that has the required flags is tried, down to plain required.
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
        let candidates =
            memory_type_candidates(types, requirements.memory_type_bits, required, preferences);
        let (&last, _) = candidates
            .split_last()
            .context("no GPU memory type supports the requested properties")?;
        for index in candidates {
            let flags = types[index as usize].property_flags;
            let info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(index);
            let memory = match unsafe { device.allocate_memory(&info, None) } {
                Ok(memory) => memory,
                Err(error) if is_out_of_memory(error) && index != last => {
                    log::warn!(
                        "{:.1} MiB in GPU memory type {index} ({flags:?}) failed: {error};                          trying the next type",
                        requirements.size as f64 / MIB
                    );
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
                unsafe { device.free_memory(memory, None) };
                return Err(error.into());
            }
            return Ok((memory, flags));
        }
        unreachable!("the last candidate returns")
    };
    match allocate() {
        Ok((memory, properties)) => Ok((buffer, memory, properties)),
        Err(error) => {
            unsafe { device.destroy_buffer(buffer, None) };
            Err(error)
        }
    }
}

const MIB: f64 = 1024.0 * 1024.0;

/// Whether an allocation failed for want of room, so another memory type
/// (another heap) may still have it.
fn is_out_of_memory(error: vk::Result) -> bool {
    matches!(
        error,
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY | vk::Result::ERROR_OUT_OF_HOST_MEMORY
    )
}

/// Every memory type allowed by `bits` that has the `required` flags, in the
/// order to try them: those with the first preference too (in index order),
/// then the second, and so on, then the rest.
fn memory_type_candidates(
    types: &[vk::MemoryType],
    bits: u32,
    required: vk::MemoryPropertyFlags,
    preferences: &[vk::MemoryPropertyFlags],
) -> Vec<u32> {
    let mut candidates = Vec::new();
    for preferred in preferences
        .iter()
        .copied()
        .chain([vk::MemoryPropertyFlags::empty()])
    {
        for (i, memory) in types.iter().enumerate() {
            let i = i as u32;
            if bits & (1 << i) != 0
                && memory.property_flags.contains(required | preferred)
                && !candidates.contains(&i)
            {
                candidates.push(i);
            }
        }
    }
    candidates
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
        let first = |bits, required, preferences: &[F]| {
            memory_type_candidates(&types, bits, required, preferences)
                .first()
                .copied()
        };
        assert_eq!(first(15, host, &[F::DEVICE_LOCAL]), Some(1));
        assert_eq!(first(13, host, &[F::DEVICE_LOCAL]), Some(0));
        assert_eq!(first(4, host, &[F::DEVICE_LOCAL]), None);
        assert_eq!(first(15, F::HOST_VISIBLE, &[F::HOST_CACHED]), Some(3));
    }

    /// A discrete GPU without Resizable BAR: device-local VRAM, system RAM,
    /// and the small host-visible window into VRAM. A vertex buffer tries the
    /// window first, then falls back to system RAM, never to VRAM the CPU
    /// can't map.
    #[test]
    fn a_full_bar_heap_falls_back_to_host_memory() {
        use vk::MemoryPropertyFlags as F;
        let host = F::HOST_VISIBLE | F::HOST_COHERENT;
        let types = [
            F::DEVICE_LOCAL,
            host,
            host | F::DEVICE_LOCAL,
            host | F::HOST_CACHED,
        ]
        .map(|property_flags| vk::MemoryType {
            property_flags,
            heap_index: 0,
        });
        assert_eq!(
            memory_type_candidates(&types, 0b1111, host, &[F::DEVICE_LOCAL]),
            [2, 1, 3]
        );
        // Types the buffer can't use are never tried.
        assert_eq!(
            memory_type_candidates(&types, 0b0101, host, &[F::DEVICE_LOCAL]),
            [2]
        );
        assert!(is_out_of_memory(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY));
        assert!(is_out_of_memory(vk::Result::ERROR_OUT_OF_HOST_MEMORY));
        assert!(!is_out_of_memory(vk::Result::ERROR_DEVICE_LOST));
    }
}
