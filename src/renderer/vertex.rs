use ash::vk;

/// Texture coordinates for untextured geometry: the shader treats any
/// negative `u` as full coverage instead of sampling the atlas.
pub const SOLID_UV: [f32; 2] = [-1.0, -1.0];

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    /// Where to sample the atlas, or `SOLID_UV`. A `u` of 1 or more marks a
    /// distance-field glyph, sampled at `u - 1`.
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 3] {
        [
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, pos) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, color) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, uv) as u32),
        ]
    }
}
