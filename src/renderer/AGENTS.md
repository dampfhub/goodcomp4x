# src/renderer/

A general-purpose 2D Vulkan renderer. It knows nothing about the game: keep game types, rules and
constants out of this directory. Its whole interface:

- `Renderer::new(window, atlas, imgui)` (unsafe: the window must outlive it), `resize`, `window_size`,
  `wait_idle`.
- `draw_frame(&[DrawBatch], Option<&DrawData>)`: each batch is a triangle list
  (`Vertex { pos, color, uv }`) with its own view-projection matrix. Optional ImGui data is
  drawn after those batches in the same render pass.
- `capture_next_frame()` then, after the next `draw_frame`, `take_captured_frame()`: that frame
  as a `Frame` (top-down RGBA8, the swapchain's sRGB-encoded bytes, opaque). Screenshots use it.
- `draw_frame(&[DrawBatch], Option<&DrawData>)`: each batch is a triangle list (`Vertex { pos, color, uv }`) with its
  own view-projection matrix, drawn in order, so later batches layer on top.

- One pipeline: alpha-blended vertex color, multiplied by the R8 atlas (bound once at set 0)
  unless the vertex's UV is `SOLID_UV` (negative `u`). The atlas holds coverage, except where
  `u` is 1 or more: those texels are a signed distance field, sampled at `u - 1` and
  thresholded at 0.5 with a one-pixel `fwidth` ramp. Text is the only atlas user today. A `u` of
  -2 or less (`soft_disc_uv`) marks a soft disc instead: `(u + 3, v)` is the place on a disc of
  radius 1, drawn solid to 0.45 and fading out at the rim (the fog's clouds use it).

## Files

| File | Role |
|---|---|
| `mod.rs` | `Renderer`: setup, `draw_frame`, swapchain recreation, teardown |
| `instance.rs` | instance, validation layer (debug builds only), debug messenger into `log` |
| `device.rs` | GPU selection, logical device, queues |
| `swapchain.rs` | swapchain, image views, and per-image render-finished semaphores |
| `pipeline.rs` | render pass (MSAA target resolved into the swapchain image), the pipeline, shader modules |
| `msaa.rs` | multisampled color target, rebuilt with the swapchain; `pick_samples` |
| `texture.rs` | the coverage atlas (R8 with mips), uploaded once |
| `buffer.rs` | buffer and memory allocation |
| `sync.rs` | per-frame acquire semaphores and fences (`MAX_FRAMES_IN_FLIGHT` = 2) |
| `readback.rs` | copying a frame's swapchain image to a host buffer, `Frame` |
| `vertex.rs` | `Vertex`, `SOLID_UV`, `soft_disc_uv` |

Shaders are GLSL in `/shaders`. `build.rs` compiles every `.vert`/`.frag`/... there to
`OUT_DIR/<name>.spv`, and `pipeline.rs` embeds them with `include_bytes!`. Adding a shader stage
needs no build change; using it needs a pipeline change here.

## Buffer memory

Each frame slot keeps its vertex buffer mapped until growth or teardown. Writes wait
for that slot's fence. Vertex memory prefers host-visible coherent device-local
memory, falling back to host-visible coherent memory. Readback prefers host-cached
memory and invalidates noncoherent mappings after the GPU finishes.

## Invariants and gotchas

- Copy SPIR-V from `include_bytes!` into a `Vec<u32>` (`create_shader_module` does). The bytes
  have no alignment guarantee; reinterpreting them in place once broke release builds only.
- `mesh.frag` samples the atlas outside the solid-geometry branch so mip selection and `fwidth`
  always have valid derivatives. Keep the sample unconditional.
- The swapchain format is sRGB and vertex colors are linear, so colors display much lighter than
  their values suggest (dark UI panels need values around 0.01-0.05).
- Readback copies the swapchain image itself, after the render pass and before presenting, in
  the same command buffer: swapchain images get `TRANSFER_SRC` usage where the surface allows
  it, and the render pass's outgoing dependency (`pipeline.rs`) makes the resolve visible to
  that copy. Reading it back swaps BGRA to RGBA; only 8-bit RGBA/BGRA formats are handled.
- The semaphore a present waits on is per swapchain image (`SwapchainData::render_finished`),
  not per frame in flight: no fence covers a present's wait, so a semaphore is only safe to
  signal again once its image is acquired again. Acquire semaphores and fences stay per frame.
  An acquire that returns suboptimal still signals its semaphore, so that frame is drawn and
  presented before the swapchain is rebuilt; only `ERROR_OUT_OF_DATE_KHR` skips the frame.
- Swapchain recreation skips zero-size surfaces, creates the replacement before releasing the old
  resources, and keeps the render pass/pipeline when the format is unchanged. Destroy methods
  null or drain handles so teardown remains safe after a partial recreation failure.
- MSAA uses the highest supported count up to 8 by default. `RENDER_MSAA=2|4|8|16|32|64`
  overrides the cap at startup (PowerShell: `$env:RENDER_MSAA="4"`). Invalid values warn
  and use 8; unsupported caps return a setup error. A 1-sample override is unsupported
  because this render pass requires a multisampled resolve source. Target allocation
  size is logged at startup and resize; the cap is retained across recreation.
- Validation runs only in debug builds (`cfg!(debug_assertions)`). Check `cargo run` output for
  validation errors after any change here.

## Device choices and atlas validation

Prefer BGRA sRGB, then RGBA sRGB, and warn if the surface forces another format.
Composite alpha uses a supported mode, preferring opaque. Atlas dimensions must be
nonzero; mip chains stop at 1x1 and clamp each axis to at least one texel. Uploads
align mip offsets to four bytes. These choices, extent clamping, batch offsets and
sample selection are exercised by `cargo test renderer` without a GPU.

## Synchronization validation

For shared-target and swapchain changes, enable synchronization validation in a debug
run. In PowerShell, set `$env:VK_VALIDATION_VALIDATE_SYNC="true"`, then run
`cargo run -- --screenshot out.png --scenario cities --size 1280x720`.
Also resize a running debug window and check that neither run logs `SYNC-HAZARD-*`
or other validation errors. Clear the variable afterwards with
`Remove-Item Env:VK_VALIDATION_VALIDATE_SYNC`. The incoming render-pass dependency
orders color attachment writes to the shared MSAA target between frames as well as
waiting for the acquired swapchain image; UNDEFINED only discards contents.
