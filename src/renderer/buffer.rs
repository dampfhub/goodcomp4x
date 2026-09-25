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
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_info, None) }?;

    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type = unsafe {
        find_memory_type(
            instance,
            physical_device,
            requirements.memory_type_bits,
            properties,
        )
    }?;
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type);
    let memory = unsafe { device.allocate_memory(&alloc_info, None) }?;
    unsafe { device.bind_buffer_memory(buffer, memory, 0) }?;

    Ok((buffer, memory))
}

/// First memory type allowed by `type_bits` that has all of `properties`.
unsafe fn find_memory_type(
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
