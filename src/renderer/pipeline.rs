use anyhow::Result;
use ash::vk;

use super::vertex::Vertex;

const VERT_SPIRV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv"));
const FRAG_SPIRV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv"));

/// Draws into a color attachment with `samples` per pixel (0), resolved at
/// the end of the pass into the swapchain image (1). No depth buffer: the
/// scene is flat, so layering is just draw order.
pub unsafe fn create_render_pass(
    device: &ash::Device,
    color_format: vk::Format,
    samples: vk::SampleCountFlags,
) -> Result<vk::RenderPass> {
    let attachments = [
        // The samples themselves are only needed until they're resolved.
        vk::AttachmentDescription::default()
            .format(color_format)
            .samples(samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
        // The resolve overwrites every pixel, so the old contents don't matter.
        vk::AttachmentDescription::default()
            .format(color_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR),
    ];

    let color_refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let resolve_refs = [vk::AttachmentReference::default()
        .attachment(1)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs)
        .resolve_attachments(&resolve_refs)];

    let dependencies = [
        incoming_color_dependency(),
        // Let a copy after the pass read the resolved image (`readback.rs`).
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ),
    ];

    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    Ok(unsafe { device.create_render_pass(&create_info, None) }?)
}

/// Alpha-blended, vertex-colored triangles with the camera's view-projection
/// matrix as a push constant, optionally masked by the coverage atlas bound
/// through `set_layout`, rasterized with `samples` per pixel to match the
/// render pass. Viewport and scissor are dynamic state.
pub unsafe fn create_graphics_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
    samples: vk::SampleCountFlags,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let vert_module = unsafe { create_shader_module(device, VERT_SPIRV) }?;
    let frag_module = unsafe { create_shader_module(device, FRAG_SPIRV) }?;
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(c"main"),
    ];

    let bindings = [Vertex::binding_description()];
    let attributes = Vertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);

    let multisampling =
        vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(samples);

    let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD)];
    let color_blending =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

    let push_constant_ranges = [vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .size(size_of::<[f32; 16]>() as u32)];
    let set_layouts = [set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }?;

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass);
    let pipeline = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_, err)| err)?[0];

    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }
    Ok((pipeline_layout, pipeline))
}

unsafe fn create_shader_module(device: &ash::Device, spirv: &[u8]) -> Result<vk::ShaderModule> {
    // `include_bytes!` data has no alignment guarantee, so copy it into a
    // `Vec<u32>` rather than reinterpreting the bytes in place.
    assert_eq!(
        spirv.len() % 4,
        0,
        "SPIR-V length must be a multiple of 4 bytes"
    );
    let code: Vec<u32> = spirv
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&word| u32::from_ne_bytes(word))
        .collect();

    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
    Ok(unsafe { device.create_shader_module(&create_info, None) }?)
}

/// Includes earlier frames' writes to the shared MSAA attachment. UNDEFINED
/// discards contents but does not eliminate its write-after-write hazard.
fn incoming_color_dependency() -> vk::SubpassDependency {
    vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_target_writes_are_ordered_between_render_passes() {
        let dependency = incoming_color_dependency();
        assert_eq!(dependency.src_subpass, vk::SUBPASS_EXTERNAL);
        assert_eq!(dependency.dst_subpass, 0);
        for stages in [dependency.src_stage_mask, dependency.dst_stage_mask] {
            assert!(stages.contains(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT));
        }
        for access in [dependency.src_access_mask, dependency.dst_access_mask] {
            assert!(access.contains(vk::AccessFlags::COLOR_ATTACHMENT_WRITE));
        }
    }
}
