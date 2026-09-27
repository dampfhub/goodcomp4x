# src/renderer/

A general-purpose 2D Vulkan renderer. It knows nothing about the game: keep game types, rules and
constants out of this directory. Its whole interface:

- `Renderer::new(window, atlas, imgui)` (unsafe: the window must outlive it), `resize`, `window_size`,
  `wait_idle`.
- `draw_frame(&[DrawBatch], Option<&DrawData>)`: each batch is a triangle list
  (`Vertex { pos, color, uv }`) with its own view-projection matrix. Optional ImGui data is
  drawn after those batches in the same render pass.
- One pipeline: alpha-blended vertex color, multiplied by the R8 atlas (bound once at set 0)
  unless the vertex's UV is `SOLID_UV` (negative `u`). The atlas holds coverage, except where
  `u` is 1 or more: those texels are a signed distance field, sampled at `u - 1` and
  thresholded at 0.5 with a one-pixel `fwidth` ramp. Text is the only atlas user today.

## Files

| File | Role |
|---|---|
| `mod.rs` | `Renderer`: setup, `draw_frame`, swapchain recreation, teardown |
| `instance.rs` | instance, validation layer (debug builds only), debug messenger into `log` |
| `device.rs` | GPU selection, logical device, queues |
| `swapchain.rs` | swapchain and image views |
| `pipeline.rs` | render pass (MSAA target resolved into the swapchain image), the pipeline, shader modules |
| `msaa.rs` | multisampled color target, rebuilt with the swapchain; `pick_samples` |
| `texture.rs` | the coverage atlas (R8 with mips), uploaded once |
| `buffer.rs` | buffer and memory allocation |
| `sync.rs` | per-frame semaphores and fences (`MAX_FRAMES_IN_FLIGHT` = 2) |
| `vertex.rs` | `Vertex`, `SOLID_UV` |

Shaders are GLSL in `/shaders`. `build.rs` compiles every `.vert`/`.frag`/... there to
`OUT_DIR/<name>.spv`, and `pipeline.rs` embeds them with `include_bytes!`. Adding a shader stage
needs no build change; using it needs a pipeline change here.

## Invariants and gotchas

- Copy SPIR-V from `include_bytes!` into a `Vec<u32>` (`create_shader_module` does). The bytes
  have no alignment guarantee; reinterpreting them in place once broke release builds only.
- `mesh.frag` samples the atlas outside the solid-geometry branch so mip selection and `fwidth`
  always have valid derivatives. Keep the sample unconditional.
- The swapchain format is sRGB and vertex colors are linear, so colors display much lighter than
  their values suggest (dark UI panels need values around 0.01-0.05).
- MSAA uses the highest supported count from `PREFERRED_SAMPLES` (16, then 8), falling back to 4.
- Validation runs only in debug builds (`cfg!(debug_assertions)`). Check `cargo run` output for
  validation errors after any change here.
