#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragUv;
layout(location = 0) out vec4 outColor;

// Single channel: coverage for UI glyphs, a signed distance field (0.5 on the
// outline, higher inside) for world glyphs.
layout(set = 0, binding = 0) uniform sampler2D atlas;
// Linear premultiplied RGBA; sprite UVs carry u + 2.
layout(set = 1, binding = 0) uniform sampler2D sprites;

void main() {
    // Untextured geometry marks itself with a negative u, distance-field
    // glyphs with u shifted up by 1, and soft discs with a u of -2 or less:
    // (u + 3, v) is then the point's place on the disc, from -1 to 1. Sample
    // outside any branch so mip selection and fwidth always have valid
    // derivatives.
    float field = step(1.0, fragUv.x);
    float sampled = texture(atlas, max(fragUv - vec2(field, 0.0), vec2(0.0))).r;
    vec4 sprite = texture(sprites, max(fragUv - vec2(2.0, 0.0), vec2(0.0)));

    // Antialias the outline over about one screen pixel, whatever the scale.
    float edge = max(fwidth(sampled) * 0.5, 1e-4);
    float inside = smoothstep(0.5 - edge, 0.5 + edge, sampled);

    // A soft disc is solid out to SOFT_DISC_SOLID of its radius and fades to
    // nothing at its rim.
    const float SOFT_DISC_SOLID = 0.45;
    float rim = length(vec2(fragUv.x + 3.0, fragUv.y));
    float soft = 1.0 - smoothstep(SOFT_DISC_SOLID, 1.0, rim);

    float coverage = fragUv.x <= -1.5 ? soft
        : fragUv.x < 0.0 ? 1.0 : mix(sampled, inside, field);
    outColor = vec4(fragColor.rgb, fragColor.a * coverage);
    if (fragUv.x >= 2.0) {
        // Filtering premultiplied texels avoids dark fringes. The pipeline
        // uses straight alpha for geometry, so undo premultiplication here.
        outColor = vec4(fragColor.rgb * sprite.rgb / max(sprite.a, 1e-6),
                        fragColor.a * sprite.a);
    }
}
