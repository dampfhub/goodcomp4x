#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragUv;
layout(location = 0) out vec4 outColor;

// Single channel: coverage for UI glyphs, a signed distance field (0.5 on the
// outline, higher inside) for world glyphs.
layout(set = 0, binding = 0) uniform sampler2D atlas;

void main() {
    // Untextured geometry marks itself with a negative u, distance-field
    // glyphs with u shifted up by 1. Sample outside any branch so mip
    // selection and fwidth always have valid derivatives.
    float field = step(1.0, fragUv.x);
    float sampled = texture(atlas, max(fragUv - vec2(field, 0.0), vec2(0.0))).r;

    // Antialias the outline over about one screen pixel, whatever the scale.
    float edge = max(fwidth(sampled) * 0.5, 1e-4);
    float inside = smoothstep(0.5 - edge, 0.5 + edge, sampled);

    float coverage = fragUv.x < 0.0 ? 1.0 : mix(sampled, inside, field);
    outColor = vec4(fragColor.rgb, fragColor.a * coverage);
}
