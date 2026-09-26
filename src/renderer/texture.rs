//! The coverage atlas: one single-channel texture every draw can sample,
//! uploaded once at startup and bound as descriptor set 0.

use anyhow::{Result, ensure};
use ash::vk;

use super::buffer;

/// A single-channel (coverage) image and its mip chain, largest level first.
/// Each level is `width >> i` by `height >> i` bytes, rows top to bottom.
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub levels: Vec<Vec<u8>>,
}

pub struct Texture {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    pub set_layout: vk::DescriptorSetLayout,
    pub set: vk::DescriptorSet,
}

impl Texture {
    /// Uploads `atlas` through a staging buffer and makes it ready to sample.
    pub unsafe fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        atlas: &Atlas,
    ) -> Result<Self> {
        let mip_levels = atlas.levels.len() as u32;
        ensure!(mip_levels > 0, "atlas has no image data");
        for (i, level) in atlas.levels.iter().enumerate() {
            let expected = (atlas.width >> i) as usize * (atlas.height >> i) as usize;
            ensure!(level.len() == expected, "atlas mip {i} has the wrong size");
        }

        let format = vk::Format::R8_UNORM;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: atlas.width,
                height: atlas.height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }?;
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type = unsafe {
            buffer::find_memory_type(
                instance,
                physical_device,
                requirements.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
        }?;
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = unsafe { device.allocate_memory(&alloc_info, None) }?;
        unsafe { device.bind_image_memory(image, memory, 0) }?;

        unsafe {
            upload(
                instance,
                device,
                physical_device,
                command_pool,
                queue,
                image,
                atlas,
            )
        }?;

        let subresource = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(mip_levels)
            .layer_count(1);
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(subresource);
        let view = unsafe { device.create_image_view(&view_info, None) }?;

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(mip_levels as f32);
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }?;

        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let set_layout = unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }?;

        let set_layouts = [set_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&set_layouts);
        let set = unsafe { device.allocate_descriptor_sets(&alloc_info) }?[0];
        let image_infos = [vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos);
        unsafe { device.update_descriptor_sets(&[write], &[]) };

        Ok(Self {
            image,
            memory,
            view,
            sampler,
            pool,
            set_layout,
            set,
        })
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

/// Copies every mip level into `image` and leaves it ready for sampling,
/// waiting for the GPU to finish before returning.
unsafe fn upload(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    atlas: &Atlas,
) -> Result<()> {
    let total: usize = atlas.levels.iter().map(Vec::len).sum();
    let (staging, staging_memory) = unsafe {
        buffer::create_buffer(
            instance,
            device,
            physical_device,
            total as vk::DeviceSize,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
    }?;

    let mut regions = Vec::with_capacity(atlas.levels.len());
    unsafe {
        let dst = device
            .map_memory(
                staging_memory,
                0,
                total as vk::DeviceSize,
                vk::MemoryMapFlags::empty(),
            )?
            .cast::<u8>();
        let mut offset = 0;
        for (i, level) in atlas.levels.iter().enumerate() {
            dst.add(offset)
                .copy_from_nonoverlapping(level.as_ptr(), level.len());
            regions.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(offset as vk::DeviceSize)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(i as u32)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: atlas.width >> i,
                        height: atlas.height >> i,
                        depth: 1,
                    }),
            );
            offset += level.len();
        }
        device.unmap_memory(staging_memory);
    }

    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer = unsafe { device.allocate_command_buffers(&alloc_info) }?[0];
    let all_levels = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(atlas.levels.len() as u32)
        .layer_count(1);
    let to_transfer = vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(all_levels)
        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
    let to_shader = vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(all_levels)
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::SHADER_READ);

    unsafe {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device.begin_command_buffer(command_buffer, &begin_info)?;
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_transfer],
        );
        device.cmd_copy_buffer_to_image(
            command_buffer,
            staging,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &regions,
        );
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_shader],
        );
        device.end_command_buffer(command_buffer)?;

        let command_buffers = [command_buffer];
        let submit = vk::SubmitInfo::default().command_buffers(&command_buffers);
        device.queue_submit(queue, &[submit], vk::Fence::null())?;
        device.queue_wait_idle(queue)?;

        device.free_command_buffers(command_pool, &command_buffers);
        device.destroy_buffer(staging, None);
        device.free_memory(staging_memory, None);
    }
    Ok(())
}
