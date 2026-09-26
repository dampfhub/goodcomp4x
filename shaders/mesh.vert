#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec4 inColor;
layout(location = 2) in vec4 inSurface;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec4 surface;
layout(location = 2) out vec2 worldPosition;

layout(push_constant) uniform PushConstants {
    mat4 viewProj;
} pc;

void main() {
    gl_Position = pc.viewProj * vec4(inPosition, 1.0);
    fragColor = inColor;
    surface = inSurface;
    worldPosition = inPosition.xy;
}
