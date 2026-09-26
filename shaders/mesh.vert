#version 450

layout(location = 0) in vec3 pos;
layout(location = 1) in vec4 color;
layout(location = 2) in vec2 uv;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragUv;

layout(push_constant) uniform Camera {
    mat4 view_proj;
} camera;

void main() {
    gl_Position = camera.view_proj * vec4(pos, 1.0);
    fragColor = color;
    fragUv = uv;
}
