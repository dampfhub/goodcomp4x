//! Reading a frame back to the CPU (for screenshots): the frame's swapchain
//! image is copied into a host-visible buffer at the end of its command
//! buffer, before it is presented.

use anyhow::{Result, bail};
use ash::vk;

use super::buffer;

/// A frame read back from the GPU: `width * height` pixels, row by row from
/// the top-left, 4 bytes each (red, green, blue, alpha). Colors are the bytes
/// the swapchain image holds, which is what the screen shows: with the usual
/// sRGB swapchain format they're already sRGB-encoded. Alpha is always 255.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A host-visible buffer one swapchain image is copied into.
pub struct Readback {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    extent: vk::Extent2D,
    bgra: bool,
    coherent: bool,
}

impl Readback {
    /// Room for one `extent`-sized image of `format`, which must be an 8-bit
    /// RGBA or BGRA format.
    pub unsafe fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        extent: vk::Extent2D,
        format: vk::Format,
    ) -> Result<Self> {
        let bgra = is_bgra(format)?;
        let size = extent.width as vk::DeviceSize * extent.height as vk::DeviceSize * 4;
        let (buffer, memory, properties) = unsafe {
            buffer::create_buffer_preferred(
                instance,
                device,
                physical_device,
                size,
                vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
                &[
                    vk::MemoryPropertyFlags::HOST_CACHED | vk::MemoryPropertyFlags::HOST_COHERENT,
                    vk::MemoryPropertyFlags::HOST_CACHED,
                    vk::MemoryPropertyFlags::HOST_COHERENT,
                ],
            )
        }?;
        Ok(Self {
            buffer,
            memory,
            extent,
            bgra,
            coherent: properties.contains(vk::MemoryPropertyFlags::HOST_COHERENT),
        })
    }

    /// Records copying `image` into the buffer. The render pass has just left
    /// `image` in `PRESENT_SRC_KHR`, with its writes made visible to transfers
    /// by the pass's outgoing dependency (`pipeline.rs`); this puts it back in
    /// that layout for presenting.
    pub unsafe fn record_copy(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        image: vk::Image,
    ) {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let to_transfer = vk::ImageMemoryBarrier::default()
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range);
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: self.extent.width,
                height: self.extent.height,
                depth: 1,
            });
        let to_present = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range);
        let to_host = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer)
            .size(vk::WHOLE_SIZE);
        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );
            device.cmd_copy_image_to_buffer(
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.buffer,
                &[region],
            );
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &[to_host],
                &[to_present],
            );
        }
    }

    /// The copied frame, as RGBA. The GPU must have finished the commands
    /// `record_copy` recorded.
    pub unsafe fn read(&self, device: &ash::Device) -> Result<Frame> {
        let len = self.extent.width as usize * self.extent.height as usize * 4;
        let rgba = unsafe {
            let data = device
                .map_memory(self.memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?
                .cast::<u8>();
            if !self.coherent {
                let range = vk::MappedMemoryRange::default()
                    .memory(self.memory)
                    .size(vk::WHOLE_SIZE);
                if let Err(error) = device.invalidate_mapped_memory_ranges(&[range]) {
                    device.unmap_memory(self.memory);
                    return Err(error.into());
                }
            }
            let rgba = to_rgba(std::slice::from_raw_parts(data, len), self.bgra);
            device.unmap_memory(self.memory);
            rgba
        };
        Ok(Frame {
            width: self.extent.width,
            height: self.extent.height,
            rgba,
        })
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }
}

/// Whether `format` stores its channels blue first (else red first), for the
/// 8-bit four-channel formats a swapchain can have.
fn is_bgra(format: vk::Format) -> Result<bool> {
    match format {
        vk::Format::B8G8R8A8_SRGB | vk::Format::B8G8R8A8_UNORM => Ok(true),
        vk::Format::R8G8B8A8_SRGB | vk::Format::R8G8B8A8_UNORM => Ok(false),
        other => bail!("can't read back a swapchain image in {other:?}"),
    }
}

/// Reorders 4-byte pixels to RGBA and makes them opaque (the swapchain is
/// presented opaque, so its alpha means nothing).
fn to_rgba(pixels: &[u8], bgra: bool) -> Vec<u8> {
    let mut rgba = pixels.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        if bgra {
            pixel.swap(0, 2);
        }
        pixel[3] = u8::MAX;
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_pixels_come_back_as_opaque_rgba() {
        let bgra = [10, 20, 30, 0, 40, 50, 60, 128];
        assert_eq!(to_rgba(&bgra, true), [30, 20, 10, 255, 60, 50, 40, 255]);
        assert_eq!(to_rgba(&bgra, false), [10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn only_8_bit_four_channel_formats_can_be_read_back() {
        assert!(is_bgra(vk::Format::B8G8R8A8_SRGB).unwrap());
        assert!(!is_bgra(vk::Format::R8G8B8A8_UNORM).unwrap());
        assert!(is_bgra(vk::Format::A2B10G10R10_UNORM_PACK32).is_err());
    }
}
