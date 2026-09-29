//! A small general-purpose 2D Vulkan renderer: each frame, the caller hands
//! over one or more triangle lists, each with its own view-projection matrix
//! (e.g. the world through a camera, then UI in screen space on top).
//! Triangles are vertex-colored, and can be masked by a single-channel
//! coverage atlas (e.g. font glyphs) supplied once at startup. Edges are
//! smoothed with multisampling, at the most samples the GPU supports.

mod buffer;
mod device;
mod instance;
mod msaa;
mod pipeline;
mod readback;
mod swapchain;
mod sync;
mod texture;
mod vertex;

use anyhow::{Context, Result};
use ash::vk;
use glam::Mat4;
use imgui::{Context as ImGuiContext, DrawData};
use imgui_rs_vulkan_renderer::{Options as ImGuiOptions, Renderer as ImGuiRenderer};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use device::QueueFamilyIndices;
use msaa::ColorTarget;
use readback::Readback;
use swapchain::SwapchainData;
use sync::{MAX_FRAMES_IN_FLIGHT, SyncObjects};
use texture::Texture;

pub use readback::Frame;
pub use texture::Atlas;
pub use vertex::{SOLID_UV, Vertex, soft_disc_uv};

/// Vertices each frame slot's buffer starts with room for. A frame that needs
/// more grows its buffer (see `write_vertices`).
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

/// All draw offsets must fit Vulkan's u32 vertex indices before any allocation.
fn pack_batches(batches: &[DrawBatch]) -> Result<(Vec<DrawRange>, usize)> {
    let mut ranges = Vec::with_capacity(batches.len());
    let mut used = 0u32;
    for batch in batches {
        let count = u32::try_from(batch.vertices.len()).context("batch exceeds u32 vertices")?;
        ranges.push(DrawRange {
            view_proj: batch.view_proj,
            first: used,
            count,
        });
        used = used
            .checked_add(count)
            .context("frame exceeds u32 vertices")?;
    }
    Ok((ranges, used as usize))
}

/// The imgui backend rotates meshes only for non-empty draw data. Track its
/// next mesh independently from the Vulkan frame slot, which rotates on every
/// submitted frame (including classic-UI frames with no ImGui vertices).
#[derive(Default)]
struct ImGuiSlotUsage {
    next_slot: usize,
    last_frame: [Option<usize>; MAX_FRAMES_IN_FLIGHT],
}

impl ImGuiSlotUsage {
    fn fence_to_wait(&self, frame: usize, nonempty: bool) -> Option<usize> {
        nonempty
            .then_some(self.last_frame[self.next_slot])
            .flatten()
            .filter(|&last_frame| last_frame != frame)
    }

    fn record(&mut self, frame: usize, nonempty: bool) {
        if nonempty {
            self.last_frame[self.next_slot] = Some(frame);
            self.next_slot = (self.next_slot + 1) % MAX_FRAMES_IN_FLIGHT;
        }
    }
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
    /// Antialiasing samples per pixel: the most the GPU supports, up to 16.
    samples: vk::SampleCountFlags,
    /// Multisampled image each frame is drawn into, then resolved into the
    /// swapchain image. Sized to match, so it's rebuilt with the swapchain.
    color_target: ColorTarget,

    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    imgui_renderer: Option<ImGuiRenderer>,
    framebuffers: Vec<vk::Framebuffer>,
    texture: Texture,

    /// One vertex buffer per frame in flight, so the CPU can fill one while
    /// the GPU still reads another, each with its capacity in vertices.
    vertex_buffers: Vec<VertexBuffer>,

    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,

    sync: SyncObjects,
    /// The in-flight fence last used with each swapchain image.
    images_in_flight: Vec<vk::Fence>,
    current_frame: usize,
    imgui_slots: ImGuiSlotUsage,

    window_size: (u32, u32),
    framebuffer_resized: bool,

    /// Set by `capture_next_frame`: the next frame drawn is also copied into
    /// `readback`.
    capture_requested: bool,
    /// Where the last captured frame was copied, until `take_captured_frame`.
    readback: Option<Readback>,
}

impl Renderer {
    /// # Safety
    /// `window` must outlive the returned renderer.
    pub unsafe fn new(window: &Window, atlas: &Atlas, imgui: &mut ImGuiContext) -> Result<Self> {
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
                vk::SwapchainKHR::null(),
            )
        }?
        .context("the initial surface has no drawable extent")?;

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

        let samples =
            unsafe { msaa::pick_samples(&vk_instance, physical_device, swapchain_data.format) };
        log::info!("antialiasing with {} samples per pixel", samples.as_raw());
        let color_target = unsafe {
            ColorTarget::new(
                &vk_instance,
                &logical_device,
                physical_device,
                swapchain_data.format,
                swapchain_data.extent,
                samples,
            )
        }?;
        let render_pass = unsafe {
            pipeline::create_render_pass(&logical_device, swapchain_data.format, samples)
        }?;
        let imgui_renderer = ImGuiRenderer::with_default_allocator(
            &vk_instance,
            physical_device,
            logical_device.clone(),
            graphics_queue,
            command_pool,
            render_pass,
            imgui,
            Some(ImGuiOptions {
                in_flight_frames: MAX_FRAMES_IN_FLIGHT,
                sample_count: samples,
                ..ImGuiOptions::default()
            }),
        )?;
        let (pipeline_layout, gfx_pipeline) = unsafe {
            pipeline::create_graphics_pipeline(
                &logical_device,
                render_pass,
                texture.set_layout,
                samples,
            )
        }?;
        let framebuffers = unsafe {
            create_framebuffers(&logical_device, render_pass, &swapchain_data, &color_target)
        }?;

        let vertex_buffers = (0..MAX_FRAMES_IN_FLIGHT)
            .map(|_| unsafe {
                create_vertex_buffer(
                    &vk_instance,
                    &logical_device,
                    physical_device,
                    VERTEX_BUFFER_CAPACITY,
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
            samples,
            color_target,
            render_pass,
            pipeline_layout,
            pipeline: gfx_pipeline,
            imgui_renderer: Some(imgui_renderer),
            framebuffers,
            texture,
            vertex_buffers,
            command_pool,
            command_buffers,
            sync,
            images_in_flight,
            current_frame: 0,
            imgui_slots: ImGuiSlotUsage::default(),
            window_size,
            framebuffer_resized: false,
            capture_requested: false,
            readback: None,
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

    /// Asks for the next frame `draw_frame` draws to also be copied back to
    /// the CPU; `take_captured_frame` returns it.
    pub fn capture_next_frame(&mut self) {
        self.capture_requested = true;
    }

    /// The frame captured since the last call (see `capture_next_frame`), or
    /// `None` if none has been drawn yet. Waits for the GPU to finish it.
    pub fn take_captured_frame(&mut self) -> Result<Option<Frame>> {
        if self.capture_requested {
            return Ok(None);
        }
        let Some(readback) = self.readback.take() else {
            return Ok(None);
        };
        self.wait_idle();
        let frame = unsafe { readback.read(&self.device) };
        unsafe { readback.destroy(&self.device) };
        frame.map(Some)
    }

    /// A fresh buffer for this frame's copy, sized for the current swapchain.
    unsafe fn prepare_readback(&mut self) -> Result<()> {
        if !self.swapchain.readable {
            anyhow::bail!("this surface's swapchain images can't be copied from");
        }
        if let Some(old) = self.readback.take() {
            // Its frame may still be in flight.
            self.wait_idle();
            unsafe { old.destroy(&self.device) };
        }
        self.readback = Some(unsafe {
            Readback::new(
                &self.instance,
                &self.device,
                self.physical_device,
                self.swapchain.extent,
                self.swapchain.format,
            )
        }?);
        Ok(())
    }

    /// Draws `batches` in order, each through its own view-projection matrix.
    pub fn draw_frame(
        &mut self,
        batches: &[DrawBatch],
        imgui_data: Option<&DrawData>,
    ) -> Result<()> {
        if self.window_size.0 == 0 || self.window_size.1 == 0 {
            return Ok(());
        }

        // Once this frame slot's fence signals, the GPU is done with its
        // vertex buffer and command buffer from last time around.
        let fence = self.sync.in_flight[self.current_frame];
        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) }?;
        let imgui_nonempty = self.imgui_renderer.is_some()
            && imgui_data.is_some_and(|data| data.total_vtx_count > 0);
        if let Some(other_frame) = self
            .imgui_slots
            .fence_to_wait(self.current_frame, imgui_nonempty)
        {
            unsafe {
                self.device
                    .wait_for_fences(&[self.sync.in_flight[other_frame]], true, u64::MAX)
            }?;
        }
        // A swapchain recreation that failed partway (`cleanup_swapchain`
        // ran, the rest didn't) left nothing to draw with: try again rather
        // than draw.
        if self.command_buffers.is_empty() {
            return unsafe { self.recreate_swapchain() };
        }
        let ranges = unsafe { self.write_vertices(batches) }?;
        // Before acquiring, so a failure here doesn't strand an acquired
        // image. A swapchain rebuilt below makes it the wrong size, but then
        // this frame isn't drawn and the next one prepares a fresh buffer.
        if self.capture_requested {
            unsafe { self.prepare_readback() }?;
        }

        let image_available = self.sync.image_available[self.current_frame];
        let acquired = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain,
                u64::MAX,
                image_available,
                vk::Fence::null(),
            )
        };
        // A suboptimal image is still acquired, and `image_available` will be
        // signaled, so it must be drawn and presented (which consumes that
        // signal) before the swapchain is rebuilt. Out of date acquires
        // nothing and signals nothing.
        let (image_index, acquire_suboptimal) = match acquired {
            Ok((index, suboptimal)) => (index as usize, suboptimal),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
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
            self.record_command_buffer(command_buffer, image_index, &ranges, imgui_data)?;
        }
        self.imgui_slots.record(self.current_frame, imgui_nonempty);

        let wait_semaphores = [image_available];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        // Indexed by image, not frame: see `SwapchainData::render_finished`.
        let signal_semaphores = [self.swapchain.render_finished[image_index]];
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
        self.capture_requested = false;

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

        if acquire_suboptimal || suboptimal || self.framebuffer_resized {
            unsafe { self.recreate_swapchain() }?;
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(())
    }

    /// Packs every batch's vertices back to back into this frame slot's vertex
    /// buffer, returning where each batch landed.
    /// Grows the buffer (to the next power of two) when the frame needs more
    /// room; the caller has already waited on this slot's fence, so the GPU is
    /// done with the old one.
    unsafe fn write_vertices(&mut self, batches: &[DrawBatch]) -> Result<Vec<DrawRange>> {
        let (ranges, used) = pack_batches(batches)?;
        if used == 0 {
            return Ok(ranges);
        }

        let capacity = self.vertex_buffers[self.current_frame].capacity;
        if used > capacity {
            let capacity = used.next_power_of_two();
            log::info!("growing vertex buffer to {capacity} vertices");
            unsafe {
                let next = create_vertex_buffer(
                    &self.instance,
                    &self.device,
                    self.physical_device,
                    capacity,
                )?;
                let old = std::mem::replace(&mut self.vertex_buffers[self.current_frame], next);
                old.destroy(&self.device);
            }
        }
        let dst = self.vertex_buffers[self.current_frame].mapped;
        unsafe {
            for (batch, range) in batches.iter().zip(&ranges) {
                dst.add(range.first as usize)
                    .copy_from_nonoverlapping(batch.vertices.as_ptr(), range.count as usize);
            }
        }
        Ok(ranges)
    }

    unsafe fn record_command_buffer(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
        ranges: &[DrawRange],
        imgui_data: Option<&DrawData>,
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
                &[self.vertex_buffers[self.current_frame].buffer],
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
            if let (Some(renderer), Some(data)) = (&mut self.imgui_renderer, imgui_data) {
                renderer.cmd_draw(command_buffer, data)?;
            }
            device.cmd_end_render_pass(command_buffer);
            if self.capture_requested
                && let Some(readback) = &self.readback
            {
                readback.record_copy(device, command_buffer, self.swapchain.images[image_index]);
            }
            device.end_command_buffer(command_buffer)?;
        }
        Ok(())
    }

    unsafe fn recreate_swapchain(&mut self) -> Result<()> {
        if self.window_size.0 == 0 || self.window_size.1 == 0 {
            return Ok(());
        }

        self.wait_idle();
        // Create the replacement before retiring the old handle. A minimized
        // surface may have a 0x0 current extent despite a nonzero cached
        // window size; in that case keep the existing resources for restore.
        let Some(next) = (unsafe {
            swapchain::create_swapchain(
                &self.device,
                &self.swapchain_loader,
                &self.surface_loader,
                self.physical_device,
                self.surface,
                self.queue_indices,
                self.window_size,
                self.swapchain.swapchain,
            )
        })?
        else {
            return Ok(());
        };
        let format_changed = next.format != self.swapchain.format;
        let next_samples = if format_changed {
            unsafe { msaa::pick_samples(&self.instance, self.physical_device, next.format) }
        } else {
            self.samples
        };
        // This backend keeps its sample count in immutable options. Never
        // make an incompatible ImGui pipeline if the surface's format changes
        // its supported sample count.
        if next_samples != self.samples {
            let mut next = next;
            unsafe { next.destroy(&self.device, &self.swapchain_loader) };
            anyhow::bail!("the surface changed its MSAA sample count; restart the renderer");
        }

        unsafe { self.cleanup_swapchain() };
        self.swapchain = next;
        if format_changed {
            unsafe { self.destroy_pipeline() };
            let new_render_pass = unsafe {
                pipeline::create_render_pass(&self.device, self.swapchain.format, next_samples)
            }?;
            if let Some(renderer) = &mut self.imgui_renderer
                && let Err(err) = renderer.set_render_pass(new_render_pass)
            {
                unsafe { self.device.destroy_render_pass(new_render_pass, None) };
                return Err(err.into());
            }
            let old_render_pass = std::mem::replace(&mut self.render_pass, new_render_pass);
            if old_render_pass != vk::RenderPass::null() {
                unsafe { self.device.destroy_render_pass(old_render_pass, None) };
            }
            (self.pipeline_layout, self.pipeline) = unsafe {
                pipeline::create_graphics_pipeline(
                    &self.device,
                    self.render_pass,
                    self.texture.set_layout,
                    next_samples,
                )
            }?;
            self.samples = next_samples;
        }
        self.color_target = unsafe {
            ColorTarget::new(
                &self.instance,
                &self.device,
                self.physical_device,
                self.swapchain.format,
                self.swapchain.extent,
                self.samples,
            )
        }?;
        self.framebuffers = unsafe {
            create_framebuffers(
                &self.device,
                self.render_pass,
                &self.swapchain,
                &self.color_target,
            )
        }?;
        self.command_buffers = unsafe {
            create_command_buffers(&self.device, self.command_pool, self.framebuffers.len())
        }?;
        self.images_in_flight = vec![vk::Fence::null(); self.swapchain.images.len()];
        self.framebuffer_resized = false;
        Ok(())
    }

    /// Sized resources only: the render pass and pipelines stay valid when
    /// the replacement surface keeps its format. Null/drain each destroyed
    /// handle, since Drop also calls this after a failed recreation.
    unsafe fn cleanup_swapchain(&mut self) {
        unsafe {
            for framebuffer in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            if !self.command_buffers.is_empty() {
                self.device
                    .free_command_buffers(self.command_pool, &self.command_buffers);
                self.command_buffers.clear();
            }
            self.color_target.destroy(&self.device);
            self.swapchain.destroy(&self.device, &self.swapchain_loader);
        }
    }

    unsafe fn destroy_pipeline(&mut self) {
        unsafe {
            let pipeline = std::mem::replace(&mut self.pipeline, vk::Pipeline::null());
            if pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(pipeline, None);
            }
            let layout = std::mem::replace(&mut self.pipeline_layout, vk::PipelineLayout::null());
            if layout != vk::PipelineLayout::null() {
                self.device.destroy_pipeline_layout(layout, None);
            }
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.wait_idle();
        self.imgui_renderer.take();
        unsafe {
            self.cleanup_swapchain();
            self.destroy_pipeline();
            let render_pass = std::mem::replace(&mut self.render_pass, vk::RenderPass::null());
            if render_pass != vk::RenderPass::null() {
                self.device.destroy_render_pass(render_pass, None);
            }
            self.texture.destroy(&self.device);
            self.sync.destroy(&self.device);
            if let Some(readback) = self.readback.take() {
                readback.destroy(&self.device);
            }
            for buffer in self.vertex_buffers.drain(..) {
                buffer.destroy(&self.device);
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
    for (chunk, value) in bytes
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(m.to_cols_array())
    {
        *chunk = value.to_ne_bytes();
    }
    bytes
}

/// Mapped for its lifetime; only write after waiting on this frame slot's fence.
struct VertexBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    capacity: usize,
    mapped: *mut Vertex,
}

impl VertexBuffer {
    /// The GPU must no longer reference this buffer.
    unsafe fn destroy(self, device: &ash::Device) {
        unsafe {
            device.unmap_memory(self.memory);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }
}

/// A host-visible vertex buffer with room for `capacity` vertices.
unsafe fn create_vertex_buffer(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    capacity: usize,
) -> Result<VertexBuffer> {
    let size = (capacity * size_of::<Vertex>()) as vk::DeviceSize;
    let (buffer, memory, _) = unsafe {
        buffer::create_buffer_preferred(
            instance,
            device,
            physical_device,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            &[vk::MemoryPropertyFlags::DEVICE_LOCAL],
        )
    }?;
    let mapped = match unsafe { device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty()) } {
        Ok(mapped) => mapped.cast::<Vertex>(),
        Err(error) => {
            unsafe {
                device.destroy_buffer(buffer, None);
                device.free_memory(memory, None);
            }
            return Err(error.into());
        }
    };
    Ok(VertexBuffer {
        buffer,
        memory,
        capacity,
        mapped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_packing_preserves_empty_batches_offsets_and_transforms() {
        let vertex = Vertex {
            pos: [0.0; 3],
            color: [1.0; 4],
            uv: SOLID_UV,
        };
        let vertices = [vertex; 6];
        let transform = Mat4::from_scale(glam::Vec3::splat(2.0));
        let batches = [
            DrawBatch {
                vertices: &vertices[..3],
                view_proj: Mat4::IDENTITY,
            },
            DrawBatch {
                vertices: &[],
                view_proj: transform,
            },
            DrawBatch {
                vertices: &vertices,
                view_proj: transform,
            },
        ];
        let (ranges, used) = pack_batches(&batches).unwrap();
        assert_eq!(used, 9);
        assert_eq!(
            ranges
                .iter()
                .map(|r| (r.first, r.count))
                .collect::<Vec<_>>(),
            [(0, 3), (3, 0), (3, 6)]
        );
        assert_eq!(ranges[0].view_proj, Mat4::IDENTITY);
        assert_eq!(ranges[2].view_proj, transform);
        let (ranges, used) = pack_batches(&[]).unwrap();
        assert!(ranges.is_empty());
        assert_eq!(used, 0);
    }

    #[test]
    fn imgui_mesh_slot_waits_for_the_other_frame_after_odd_empty_frames() {
        let mut slots = ImGuiSlotUsage::default();
        assert_eq!(slots.fence_to_wait(0, true), None);
        slots.record(0, true);
        assert_eq!(slots.fence_to_wait(1, true), None);
        slots.record(1, true);
        assert_eq!(slots.fence_to_wait(0, false), None);
        slots.record(0, false);
        // Renderer frame 0 ran without ImGui, while ImGui's next mesh stayed 0.
        assert_eq!(slots.fence_to_wait(1, true), Some(0));
        slots.record(1, true);
        assert_eq!(slots.fence_to_wait(0, true), Some(1));
    }

    #[test]
    fn mat4_bytes_are_the_columns_in_order() {
        let m = Mat4::from_cols_array(&std::array::from_fn(|i| i as f32));
        let bytes = mat4_to_bytes(&m);
        for i in 0..16 {
            let word: [u8; 4] = bytes[4 * i..4 * i + 4].try_into().unwrap();
            assert_eq!(f32::from_ne_bytes(word), i as f32);
        }
    }
}
