//! Compiles each GLSL shader in `shaders/` to `OUT_DIR/<file name>.spv` with
//! the Vulkan SDK's `glslc`; the renderer embeds them with `include_bytes!`.
//! On Windows it also embeds the game's icon in the executable.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "src/icon_art.rs"]
mod icon_art;

const SHADER_EXTENSIONS: [&str; 6] = ["vert", "frag", "comp", "geom", "tesc", "tese"];
/// The icon sizes embedded in the Windows executable.
const EXE_ICON_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

fn main() {
    let shader_dir = Path::new("shaders");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    embed_icon(&out_dir);
    embed_sprites(&out_dir);
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

/// Lists every PNG under `assets/sprites/` as `(tag, bytes)` in
/// `OUT_DIR/embedded_sprites.rs`, so the game has the shipped sprites without
/// the folder (`pixel_art.rs`). A sprite's tag is its path there without
/// `.png`, with forward slashes.
fn embed_sprites(out_dir: &Path) {
    let root = Path::new("assets/sprites");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect_pngs(root, &mut files);
    files.sort();
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut list = String::from("&[\n");
    for path in files {
        let tag = path
            .strip_prefix(root)
            .unwrap()
            .with_extension("")
            .components()
            .map(|part| part.as_os_str().to_str().expect("sprite paths are UTF-8"))
            .collect::<Vec<_>>()
            .join("/");
        let absolute = manifest.join(&path);
        list += &format!("    ({tag:?}, include_bytes!({absolute:?})),\n");
    }
    list += "]\n";
    std::fs::write(out_dir.join("embedded_sprites.rs"), list)
        .expect("failed to write embedded_sprites.rs");
}

fn collect_pngs(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("failed to read assets/sprites/").path();
        if path.is_dir() {
            collect_pngs(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "png") {
            files.push(path);
        }
    }
}

/// On a Windows (MSVC) build, writes the icon as a compiled resource file and
/// has the linker embed it in the executable. The window also sets its icon
/// at runtime, but Windows sometimes shows the taskbar button with the
/// executable's own icon, which would otherwise be the blank default.
fn embed_icon(out_dir: &Path) {
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") != "windows" || target("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let res = out_dir.join("icon.res");
    std::fs::write(&res, icon_art::windows_res(&EXE_ICON_SIZES)).expect("failed to write icon.res");
    // The MSVC linker takes a .res file as an input and embeds it.
    println!("cargo:rustc-link-arg-bins={}", res.display());
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
