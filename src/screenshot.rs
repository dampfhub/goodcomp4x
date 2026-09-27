//! Screenshot mode (`--screenshot out.png`): draw a few frames so the scene
//! settles, read one back from the renderer, write it as a PNG, and quit. It
//! lets someone (or an agent) without a view of the window check what a
//! change looks like.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::renderer::{Frame, Renderer};

/// Window size for screenshots when `--size` isn't given, so they don't
/// depend on the monitor.
pub const DEFAULT_SIZE: (u32, u32) = (1600, 900);
/// Frames drawn, and time passed, before the one captured: long enough for
/// the swapchain to settle and a camera glide (`camera.rs`) to finish.
const SETTLE_FRAMES: u32 = 5;
const SETTLE_TIME: Duration = Duration::from_secs(1);
/// Giving up: something kept frames from being drawn or read back.
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Screenshot {
    path: PathBuf,
    /// When the app started, for the timeout.
    started: Instant,
    /// When the first frame was drawn, for settling.
    first_frame: Option<Instant>,
    frames_drawn: u32,
    requested: bool,
    written: bool,
}

impl Screenshot {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            started: Instant::now(),
            first_frame: None,
            frames_drawn: 0,
            requested: false,
            written: false,
        }
    }

    pub fn is_written(&self) -> bool {
        self.written
    }

    /// Call after each frame is drawn. Asks the renderer to capture the next
    /// frame once the scene has settled, then writes that frame out. Returns
    /// whether the screenshot is written.
    pub fn after_frame(&mut self, renderer: &mut Renderer) -> Result<bool> {
        self.frames_drawn += 1;
        let first_frame = *self.first_frame.get_or_insert_with(Instant::now);
        if !self.requested {
            if self.frames_drawn >= SETTLE_FRAMES && first_frame.elapsed() >= SETTLE_TIME {
                renderer.capture_next_frame();
                self.requested = true;
            }
            return Ok(false);
        }
        let Some(frame) = renderer.take_captured_frame()? else {
            return Ok(false);
        };
        save_png(&self.path, &frame)?;
        self.written = true;
        log::info!(
            "wrote a {}x{} screenshot to {}",
            frame.width,
            frame.height,
            self.path.display()
        );
        Ok(true)
    }

    /// An error once it's taken too long, so a stuck run still exits.
    pub fn check_timeout(&self) -> Result<()> {
        if self.started.elapsed() > TIMEOUT {
            bail!(
                "no screenshot after {} s ({} frames drawn)",
                TIMEOUT.as_secs(),
                self.frames_drawn
            );
        }
        Ok(())
    }
}

fn save_png(path: &Path, frame: &Frame) -> Result<()> {
    let file = File::create(path).with_context(|| format!("can't create {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&frame.rgba)?;
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_png_reads_back_the_same() {
        let frame = Frame {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 10, 20, 30, 255],
        };
        let path = std::env::temp_dir().join(format!("screenshot-test-{}.png", std::process::id()));
        save_png(&path, &frame).unwrap();

        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&pixels[..info.buffer_size()], &frame.rgba[..]);
    }
}
