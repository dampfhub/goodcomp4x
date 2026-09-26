//! A small general-purpose 2D Vulkan renderer: each frame, the caller hands
//! over one or more triangle lists, each with its own view-projection matrix
//! (e.g. the world through a camera, then UI in screen space on top).
//! Triangles are vertex-colored, and can be masked by a single-channel
//! coverage atlas (e.g. font glyphs) supplied once at startup. Edges are
//! smoothed with 4x multisampling.

mod buffer;
mod device;
mod instance;
mod msaa;
mod pipeline;
mod swapchain;
mod sync;
mod texture;
mod vertex;

use anyhow::Result;
use ash::vk;
use glam::Mat4;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use device::QueueFamilyIndices;
use msaa::ColorTarget;
use swapchain::SwapchainData;
use sync::{MAX_FRAMES_IN_FLIGHT, SyncObjects};
use texture::Texture;

pub use texture::Atlas;
pub use vertex::{SOLID_UV, Vertex};

/// Most vertices one frame can draw; anything past this is dropped with a warning.
const VERTEX_BUFFER_CAPACITY: usize = 65_536;

const CLEAR_COLOR: [f32; 4] = [0.06, 0.06, 0.08, 1.0];

/// A triangle list and the view-projection matrix to draw it with. Batches
/// are drawn in order, so later ones layer on top.
pub struct DrawBatch<'a> {
    pub view_proj: Mat4,
    pub vertices: &'a [Vertex],
}

/// Where a batch's vertices landed in the frame's vertex buffer.
struct DrawRange {
    view_proj: Mat4,
    first: u32,
    count: u32,
}

pub struct Renderer {
    _entry: ash::Entry,
    instance: ash::Instance,
    debug_messenger: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,

    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,

    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    queue_indices: QueueFamilyIndices,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,

    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: SwapchainData,
    /// Multisampled image each frame is drawn into, then resolved into the
    /// swapchain image. Sized to match, so it's rebuilt with the swapchain.
    color_target: ColorTarget,

    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    framebuffers: Vec<vk::Framebuffer>,
    texture: Texture,

    /// One vertex buffer per frame in flight, so the CPU can fill one while
    /// the GPU still reads another.
    vertex_buffers: Vec<(vk::Buffer, vk::DeviceMemory)>,

    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,

    sync: SyncObjects,
    /// The in-flight fence last used with each swapchain image.
    images_in_flight: Vec<vk::Fence>,
    current_frame: usize,

    window_size: (u32, u32),
    framebuffer_resized: bool,
}

impl Renderer {
    /// # Safety
    /// `window` must outlive the returned renderer.
    pub unsafe fn new(window: &Window, atlas: &Atlas) -> Result<Self> {
        let display_handle = window.display_handle()?.as_raw();
        let window_handle = window.window_handle()?.as_raw();
        let window_size = window.inner_size().into();

        let entry = unsafe { instance::create_entry() }?;
        let vk_instance = unsafe { instance::create_instance(&entry, display_handle) }?;
        let debug_messenger = if instance::validation_enabled() {
            Some(unsafe { instance::create_debug_messenger(&entry, &vk_instance) }?)
        } else {
            None
        };

        let surface_loader = ash::khr::surface::Instance::new(&entry, &vk_instance);
        let surface = unsafe {
            ash_window::create_surface(&entry, &vk_instance, display_handle, window_handle, None)
        }?;

        let (physical_device, queue_indices) =
            unsafe { device::pick_physical_device(&vk_instance, &surface_loader, surface) }?;
        let (logical_device, graphics_queue, present_queue) =
            unsafe { device::create_logical_device(&vk_instance, physical_device, queue_indices) }?;

        let swapchain_loader = ash::khr::swapchain::Device::new(&vk_instance, &logical_device);
        let swapchain_data = unsafe {
            swapchain::create_swapchain(
                &logical_device,
                &swapchain_loader,
                &surface_loader,
                physical_device,
                surface,
                queue_indices,
                window_size,
            )
        }?;

        let command_pool = unsafe { create_command_pool(&logical_device, queue_indices) }?;
        let texture = unsafe {
            Texture::new(
                &vk_instance,
                &logical_device,
                physical_device,
                command_pool,
                graphics_queue,
                atlas,
            )
        }?;

        let color_target = unsafe {
            ColorTarget::new(
                &vk_instance,
                &logical_device,
                physical_device,
                swapchain_data.format,
                swapchain_data.extent,
            )
        }?;
        let render_pass =
            unsafe { pipeline::create_render_pass(&logical_device, swapchain_data.format) }?;
        let (pipeline_layout, gfx_pipeline) = unsafe {
            pipeline::create_graphics_pipeline(&logical_device, render_pass, texture.set_layout)
        }?;
        let framebuffers = unsafe {
            create_framebuffers(&logical_device, render_pass, &swapchain_data, &color_target)
        }?;

        let vertex_buffer_size = (VERTEX_BUFFER_CAPACITY * size_of::<Vertex>()) as vk::DeviceSize;
        let vertex_buffers = (0..MAX_FRAMES_IN_FLIGHT)
            .map(|_| unsafe {
                buffer::create_buffer(
                    &vk_instance,
                    &logical_device,
                    physical_device,
                    vertex_buffer_size,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        let command_buffers =
            unsafe { create_command_buffers(&logical_device, command_pool, framebuffers.len()) }?;

        let sync = unsafe { sync::create_sync_objects(&logical_device) }?;
        let images_in_flight = vec![vk::Fence::null(); swapchain_data.images.len()];

        Ok(Self {
            _entry: entry,
            instance: vk_instance,
            debug_messenger,
            surface_loader,
            surface,
            physical_device,
            device: logical_device,
            queue_indices,
            graphics_queue,
            present_queue,
            swapchain_loader,
            swapchain: swapchain_data,
            color_target,
            render_pass,
            pipeline_layout,
            pipeline: gfx_pipeline,
            framebuffers,
            texture,
            vertex_buffers,
            command_pool,
            command_buffers,
            sync,
            images_in_flight,
            current_frame: 0,
            window_size,
            framebuffer_resized: false,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.window_size = (width, height);
        self.framebuffer_resized = true;
    }

    pub fn window_size(&self) -> (u32, u32) {
        self.window_size
    }

    pub fn wait_idle(&self) {
        let _ = unsafe { self.device.device_wait_idle() };
    }

    /// Draws `batches` in order, each through its own view-projection matrix.
    pub fn draw_frame(&mut self, batches: &[DrawBatch]) -> Result<()> {
        if self.window_size.0 == 0 || self.window_size.1 == 0 {
            return Ok(());
        }

        // Once this frame slot's fence signals, the GPU is done with its
        // vertex buffer and command buffer from last time around.
        let fence = self.sync.in_flight[self.current_frame];
        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) }?;
        let ranges = unsafe { self.write_vertices(batches) }?;

        let image_available = self.sync.image_available[self.current_frame];
        let acquired = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain,
                u64::MAX,
                image_available,
                vk::Fence::null(),
            )
        };
        let image_index = match acquired {
            Ok((index, false)) => index as usize,
            Ok((_, true)) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return unsafe { self.recreate_swapchain() };
            }
            Err(err) => return Err(err.into()),
        };

        // A previous frame may still be rendering to this same image.
        let image_fence = self.images_in_flight[image_index];
        if image_fence != vk::Fence::null() {
            unsafe { self.device.wait_for_fences(&[image_fence], true, u64::MAX) }?;
        }
        self.images_in_flight[image_index] = fence;

        let command_buffer = self.command_buffers[image_index];
        unsafe {
            self.device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.record_command_buffer(command_buffer, image_index, &ranges)?;
        }

        let wait_semaphores = [image_available];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = [self.sync.render_finished[self.current_frame]];
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        unsafe {
            self.device.reset_fences(&[fence])?;
            self.device
                .queue_submit(self.graphics_queue, &[submit_info], fence)?;
        }

        let swapchains = [self.swapchain.swapchain];
        let image_indices = [image_index as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        let suboptimal = match unsafe {
            self.swapchain_loader
                .queue_present(self.present_queue, &present_info)
        } {
            Ok(suboptimal) => suboptimal,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
            Err(err) => return Err(err.into()),
        };

        if suboptimal || self.framebuffer_resized {
            self.framebuffer_resized = false;
            unsafe { self.recreate_swapchain() }?;
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(())
    }

    /// Packs every batch's vertices back to back into this frame slot's vertex
    /// buffer, returning where each batch landed.
    unsafe fn write_vertices(&mut self, batches: &[DrawBatch]) -> Result<Vec<DrawRange>> {
        let mut ranges = Vec::with_capacity(batches.len());
        let mut used = 0;
        for batch in batches {
            let count = batch.vertices.len().min(VERTEX_BUFFER_CAPACITY - used);
            if count < batch.vertices.len() {
                log::warn!(
                    "dropping {} vertices past buffer capacity",
                    batch.vertices.len() - count
                );
            }
            ranges.push(DrawRange {
                view_proj: batch.view_proj,
                first: used as u32,
                count: count as u32,
            });
            used += count;
        }
        if used == 0 {
            return Ok(ranges);
        }

        let (_, memory) = self.vertex_buffers[self.current_frame];
        let size = (used * size_of::<Vertex>()) as vk::DeviceSize;
        unsafe {
            let dst = self
                .device
                .map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?
                .cast::<Vertex>();
            for (batch, range) in batches.iter().zip(&ranges) {
                dst.add(range.first as usize)
                    .copy_from_nonoverlapping(batch.vertices.as_ptr(), range.count as usize);
            }
            self.device.unmap_memory(memory);
        }
        Ok(ranges)
    }

    unsafe fn record_command_buffer(
        &self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
        ranges: &[DrawRange],
    ) -> Result<()> {
        let extent = self.swapchain.extent;
        let full_area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: CLEAR_COLOR,
            },
        }];
        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index])
            .render_area(full_area)
            .clear_values(&clear_values);
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        let device = &self.device;
        unsafe {
            device.begin_command_buffer(command_buffer, &vk::CommandBufferBeginInfo::default())?;
            device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.texture.set],
                &[],
            );
            device.cmd_bind_vertex_buffers(
                command_buffer,
                0,
                &[self.vertex_buffers[self.current_frame].0],
                &[0],
            );
            device.cmd_set_viewport(command_buffer, 0, &[viewport]);
            device.cmd_set_scissor(command_buffer, 0, &[full_area]);
            for range in ranges.iter().filter(|r| r.count > 0) {
                device.cmd_push_constants(
                    command_buffer,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    &mat4_to_bytes(&range.view_proj),
                );
                device.cmd_draw(command_buffer, range.count, 1, range.first, 0);
            }
            device.cmd_end_render_pass(command_buffer);
            device.end_command_buffer(command_buffer)?;
        }
        Ok(())
    }

    unsafe fn recreate_swapchain(&mut self) -> Result<()> {
        if self.window_size.0 == 0 || self.window_size.1 == 0 {
            return Ok(());
        }

        self.wait_idle();
        unsafe {
            self.cleanup_swapchain();
            self.swapchain = swapchain::create_swapchain(
                &self.device,
                &self.swapchain_loader,
                &self.surface_loader,
                self.physical_device,
                self.surface,
                self.queue_indices,
                self.window_size,
            )?;
            self.color_target = ColorTarget::new(
                &self.instance,
                &self.device,
                self.physical_device,
                self.swapchain.format,
                self.swapchain.extent,
            )?;
            self.render_pass = pipeline::create_render_pass(&self.device, self.swapchain.format)?;
            (self.pipeline_layout, self.pipeline) = pipeline::create_graphics_pipeline(
                &self.device,
                self.render_pass,
                self.texture.set_layout,
            )?;
            self.framebuffers = create_framebuffers(
                &self.device,
                self.render_pass,
                &self.swapchain,
                &self.color_target,
            )?;
            self.command_buffers =
                create_command_buffers(&self.device, self.command_pool, self.framebuffers.len())?;
        }
        self.images_in_flight = vec![vk::Fence::null(); self.swapchain.images.len()];
        Ok(())
    }

    /// Destroys everything that depends on the swapchain's size or format.
    unsafe fn cleanup_swapchain(&mut self) {
        unsafe {
            for framebuffer in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.color_target.destroy(&self.device);
            self.swapchain.destroy(&self.device, &self.swapchain_loader);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.wait_idle();
        unsafe {
            self.cleanup_swapchain();
            self.texture.destroy(&self.device);
            self.sync.destroy(&self.device);
            for &(buffer, memory) in &self.vertex_buffers {
                self.device.destroy_buffer(buffer, None);
                self.device.free_memory(memory, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((loader, messenger)) = self.debug_messenger.take() {
                loader.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// One framebuffer per swapchain image, each pairing the shared multisampled
/// target with that image to resolve into.
unsafe fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    swapchain: &SwapchainData,
    color_target: &ColorTarget,
) -> Result<Vec<vk::Framebuffer>> {
    swapchain
        .image_views
        .iter()
        .map(|&view| {
            let attachments = [color_target.view, view];
            let create_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(swapchain.extent.width)
                .height(swapchain.extent.height)
                .layers(1);
            Ok(unsafe { device.create_framebuffer(&create_info, None) }?)
        })
        .collect()
}

unsafe fn create_command_pool(
    device: &ash::Device,
    indices: QueueFamilyIndices,
) -> Result<vk::CommandPool> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(indices.graphics);
    Ok(unsafe { device.create_command_pool(&create_info, None) }?)
}

unsafe fn create_command_buffers(
    device: &ash::Device,
    pool: vk::CommandPool,
    count: usize,
) -> Result<Vec<vk::CommandBuffer>> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count as u32);
    Ok(unsafe { device.allocate_command_buffers(&alloc_info) }?)
}

fn mat4_to_bytes(m: &Mat4) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, value) in bytes.chunks_exact_mut(4).zip(m.to_cols_array()) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}
