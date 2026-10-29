//! Runtime controls cover the concept image's static loading controls. The
//! supplied artwork stays unchanged; all progress and information below are live.
use super::{Phase, Progress};
use fontdue::{Font, FontSettings};

const GOLD: u32 = 0xe9ad54;
const IVORY: u32 = 0xf0dfb4;
const MUTED: u32 = 0x918777;
const PANEL: u32 = 0x08090f;

pub(super) struct Painter {
    art: image::RgbImage,
    font: Font,
    background: Vec<u32>,
    dimensions: (u32, u32),
}
impl Painter {
    pub(super) fn new() -> Result<Self, crate::Error> {
        Ok(Self {
            art: image::load_from_memory(include_bytes!("../../assets/startup-loading.png"))?
                .to_rgb8(),
            font: Font::from_bytes(bevy::text::DEFAULT_FONT_DATA, FontSettings::default())
                .map_err(str::to_owned)?,
            background: Vec::new(),
            dimensions: (0, 0),
        })
    }
    pub(super) fn render(&mut self, width: u32, height: u32, progress: &Progress) -> Vec<u32> {
        if self.dimensions != (width, height) {
            let art = image::imageops::resize(
                &self.art,
                width,
                height,
                image::imageops::FilterType::Triangle,
            );
            self.background = art
                .pixels()
                .map(|p| (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]))
                .collect();
            for y in 0..height {
                let opacity = ((y as f32 / height as f32 - 0.68) / 0.045).clamp(0.0, 1.0);
                for x in 0..width {
                    let i = (y * width + x) as usize;
                    self.background[i] = blend(self.background[i], PANEL, opacity);
                }
            }
            self.dimensions = (width, height);
        }
        let mut canvas = Canvas {
            width: width as i32,
            height: height as i32,
            pixels: self.background.clone(),
        };
        let w = width as f32;
        let h = height as f32;
        let scale = w / 1152.0;
        canvas.text(
            &self.font,
            progress
                .error
                .as_ref()
                .map_or(progress.phase.label(), |_| "Unable to enter the dungeon"),
            w * 0.5,
            h * 0.758,
            24.0 * scale,
            IVORY,
        );
        let left = (w * 0.18) as i32;
        let right = (w * 0.82) as i32;
        let top = (h * 0.792) as i32;
        let bar_h = (18.0 * scale).max(8.0) as i32;
        canvas.rect(left - 5, top - 4, right + 5, top + bar_h + 4, 0x684923);
        canvas.rect(left - 3, top - 2, right + 3, top + bar_h + 2, GOLD);
        canvas.rect(left - 1, top, right + 1, top + bar_h, 0x15151b);
        let end = left + (right - left) * i32::from(progress.phase.completed()) / 4;
        for x in left + 2..end - 2 {
            let t = (x - left) as f32 / (right - left) as f32;
            canvas.rect(
                x,
                top + 2,
                x + 1,
                top + bar_h - 2,
                blend(0xd47526, 0xffdc8b, t),
            );
        }
        canvas.diamond(left - 12, top + bar_h / 2, (9.0 * scale) as i32, 0x9c713a);
        canvas.diamond(right + 12, top + bar_h / 2, (9.0 * scale) as i32, 0x9c713a);
        if progress.phase.completed() > 0 {
            canvas.diamond(
                end.clamp(left + 3, right - 3),
                top + bar_h / 2,
                (7.0 * scale) as i32,
                0xffda91,
            );
        }
        let labels = [
            ("World ready", progress.phase >= Phase::Entering),
            (
                "Preparing visuals",
                progress.phase >= Phase::Renderer && progress.phase < Phase::Ready,
            ),
            ("Entering dungeon", progress.phase == Phase::Ready),
        ];
        for (i, (label, active)) in labels.into_iter().enumerate() {
            let x = w * (0.28 + i as f32 * 0.22);
            let color = if active { GOLD } else { MUTED };
            canvas.text(&self.font, label, x, h * 0.871, 14.0 * scale, color);
            canvas.diamond(
                (x - 81.0 * scale) as i32,
                (h * 0.864) as i32,
                (4.0 * scale).max(2.0) as i32,
                color,
            );
        }
        let detail = progress.error.as_deref().unwrap_or(&progress.detail);
        let detail = if detail.chars().count() > 105 {
            format!("{}...", detail.chars().take(102).collect::<String>())
        } else {
            detail.to_owned()
        };
        canvas.text(&self.font, &detail, w * 0.5, h * 0.908, 12.0 * scale, MUTED);
        canvas.rect(
            (w * 0.25) as i32,
            (h * 0.929) as i32,
            (w * 0.75) as i32,
            (h * 0.929) as i32 + 1,
            0x6a4d2d,
        );
        canvas.text(
            &self.font,
            "Exploration tip: Q / E rotates the camera.",
            w * 0.5,
            h * 0.963,
            16.0 * scale,
            IVORY,
        );
        canvas.pixels
    }
}

fn blend(a: u32, b: u32, t: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |out, shift| {
        let a = ((a >> shift) & 255) as f32;
        let b = ((b >> shift) & 255) as f32;
        out | (((a + (b - a) * t).round() as u32) << shift)
    })
}

struct Canvas {
    width: i32,
    height: i32,
    pixels: Vec<u32>,
}
impl Canvas {
    fn rect(&mut self, left: i32, top: i32, right: i32, bottom: i32, color: u32) {
        for y in top.max(0)..bottom.min(self.height) {
            for x in left.max(0)..right.min(self.width) {
                self.pixels[(y * self.width + x) as usize] = color;
            }
        }
    }
    fn diamond(&mut self, x: i32, y: i32, radius: i32, color: u32) {
        for dy in -radius..=radius {
            let dx = radius - dy.abs();
            self.rect(x - dx, y + dy, x + dx + 1, y + dy + 1, color);
        }
    }
    fn text(&mut self, font: &Font, text: &str, center: f32, baseline: f32, size: f32, color: u32) {
        let width: f32 = text
            .chars()
            .map(|c| font.metrics(c, size).advance_width)
            .sum();
        let mut pen = center - width * 0.5;
        for c in text.chars() {
            let (metrics, bitmap) = font.rasterize(c, size);
            let left = pen.round() as i32 + metrics.xmin;
            let top = baseline.round() as i32 - metrics.height as i32 - metrics.ymin;
            for y in 0..metrics.height {
                for x in 0..metrics.width {
                    let (dx, dy) = (left + x as i32, top + y as i32);
                    if dx >= 0 && dy >= 0 && dx < self.width && dy < self.height {
                        let i = (dy * self.width + dx) as usize;
                        self.pixels[i] = blend(
                            self.pixels[i],
                            color,
                            f32::from(bitmap[y * metrics.width + x]) / 255.0,
                        );
                    }
                }
            }
            pen += metrics.advance_width;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_bar_reflects_zero_half_and_complete_milestones() {
        let mut painter = Painter::new().unwrap();
        let mut progress = Progress::default();
        let start = painter.render(800, 500, &progress);
        progress.advance(Phase::Observation, "graphics ready".into());
        let half = painter.render(800, 500, &progress);
        progress.advance(Phase::Ready, "ready".into());
        let full = painter.render(800, 500, &progress);
        let point = |pixels: &[u32], x: usize| pixels[403 * 800 + x];
        assert_eq!(point(&start, 250), 0x15151b);
        assert_ne!(point(&half, 250), point(&start, 250));
        assert_eq!(point(&half, 600), 0x15151b);
        assert_ne!(point(&full, 600), point(&half, 600));
        assert_eq!(
            start[50 * 800 + 400],
            full[50 * 800 + 400],
            "progress cannot alter the supplied title artwork"
        );
    }
    #[test]
    fn the_concepts_static_loading_controls_are_covered_and_small_windows_clip_safely() {
        let mut painter = Painter::new().unwrap();
        let frame = painter.render(800, 500, &Progress::default());
        assert_eq!(frame[390 * 800 + 40], PANEL);
        assert_eq!(frame.len(), 800 * 500);
        assert_eq!(
            painter.render(320, 200, &Progress::default()).len(),
            320 * 200
        );
    }
}
