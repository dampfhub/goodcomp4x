//! Compiles each GLSL shader in `shaders/` to `OUT_DIR/<file name>.spv` with
//! the Vulkan SDK's `glslc`; the renderer embeds them with `include_bytes!`.

use std::path::{Path, PathBuf};
use std::process::Command;

const SHADER_EXTENSIONS: [&str; 6] = ["vert", "frag", "comp", "geom", "tesc", "tese"];

fn main() {
    let shader_dir = Path::new("shaders");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let glslc = find_glslc();

    println!("cargo:rerun-if-changed={}", shader_dir.display());

    for entry in std::fs::read_dir(shader_dir).expect("failed to read shaders/") {
        let path = entry.unwrap().path();
        let is_shader = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| SHADER_EXTENSIONS.contains(&ext));
        if !is_shader {
            continue;
        }

        let out_path = out_dir.join(format!(
            "{}.spv",
            path.file_name().unwrap().to_str().unwrap()
        ));
        let status = Command::new(&glslc)
            .arg(&path)
            .arg("-o")
            .arg(&out_path)
            .status()
            .unwrap_or_else(|err| panic!("failed to run {}: {err}", glslc.display()));
        assert!(
            status.success(),
            "glslc failed to compile {}",
            path.display()
        );
    }
}

/// `glslc` from `$VULKAN_SDK` if it's there, otherwise whatever is on `PATH`.
fn find_glslc() -> PathBuf {
    let exe = if cfg!(windows) { "glslc.exe" } else { "glslc" };
    if let Ok(sdk) = std::env::var("VULKAN_SDK") {
        for bin_dir in ["Bin", "bin"] {
            let candidate = Path::new(&sdk).join(bin_dir).join(exe);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(exe)
}
