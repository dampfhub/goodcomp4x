#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragUv;
layout(location = 0) out vec4 outColor;

// Single-channel coverage, e.g. font glyphs.
layout(set = 0, binding = 0) uniform sampler2D atlas;

void main() {
    // Untextured geometry marks itself with a negative u. Sample outside the
    // branch so mip selection always has valid derivatives.
    float sampled = texture(atlas, max(fragUv, vec2(0.0))).r;
    float coverage = fragUv.x < 0.0 ? 1.0 : sampled;
    outColor = vec4(fragColor.rgb, fragColor.a * coverage);
}
