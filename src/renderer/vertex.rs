use ash::vk;

/// Texture coordinates for untextured geometry: the shader treats any
/// negative `u` as full coverage instead of sampling the atlas.
pub const SOLID_UV: [f32; 2] = [-1.0, -1.0];

/// Texture coordinates for a soft disc drawn on a quad: `local` is the
/// vertex's place on the disc, each coordinate from -1 to 1. The shader
/// draws the disc solid out to almost half its radius, fading to
/// transparent at the rim, and nothing outside it.
pub fn soft_disc_uv(local: [f32; 2]) -> [f32; 2] {
    [local[0] - 3.0, local[1]]
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    /// Where to sample the atlas, or `SOLID_UV`. `1 <= u < 2` marks a
    /// distance-field glyph at `u - 1`; `2 <= u < 3` a color sprite at
    /// `u - 2`; a `u` of -2 or less, a soft disc (`soft_disc_uv`).
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
