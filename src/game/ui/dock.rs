//! Geometry-only docking for screen-space panels. Callers provide measured
//! panel sizes; placement is shared by rendering and hit testing in `mod.rs`.

use glam::Vec2;

#[allow(dead_code)] // All four zones are part of the UI extension API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Zone {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
    /// The middle of the bottom edge, sliding sideways to the nearest free
    /// spot before stacking upward (`Dock::place`).
    BottomCenter,
}

impl Zone {
    fn left(self) -> bool {
        matches!(self, Self::BottomLeft | Self::TopLeft)
    }

    fn bottom(self) -> bool {
        matches!(
            self,
            Self::BottomLeft | Self::BottomRight | Self::BottomCenter
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub(super) fn overlaps(self, other: Self, gap: f32) -> bool {
        self.min.x < other.max.x + gap
            && self.max.x + gap > other.min.x
            && self.min.y < other.max.y + gap
            && self.max.y + gap > other.min.y
    }

    fn overlaps_horizontally(self, other: Self, gap: f32) -> bool {
        self.min.x < other.max.x + gap && self.max.x + gap > other.min.x
    }
}

/// Tracks panels already placed inside a screen's safe area. Each zone starts
/// in its named corner, stacks vertically, then wraps to another column if
/// that column runs out of room. All zones share collision information.
pub(super) struct Dock {
    screen: Vec2,
    margin: f32,
    gap: f32,
    top_reserved: f32,
    panels: Vec<Rect>,
}

impl Dock {
    pub fn new(screen: Vec2, margin: f32, gap: f32, top_reserved: f32) -> Self {
        Self {
            screen,
            margin,
            gap,
            top_reserved,
            panels: Vec::new(),
        }
    }

    /// Reserve a window the user moved, so automatically placed windows avoid it.
    pub(super) fn reserve(&mut self, rect: Rect) {
        self.panels.push(rect);
    }

    fn top(&self) -> f32 {
        self.screen.y - self.top_reserved
    }

    /// Vertical room remaining in the anchor column for a panel of `width`.
    /// Useful for panels that choose how many rows to render before docking.
    pub fn remaining_height(&self, zone: Zone, width: f32) -> f32 {
        let x = if zone == Zone::BottomCenter {
            (self.screen.x - width) / 2.0
        } else if zone.left() {
            self.margin
        } else {
            self.screen.x - self.margin - width
        };
        let probe = Rect {
            min: Vec2::new(x, self.margin),
            max: Vec2::new(x + width, self.top()),
        };
        if zone.bottom() {
            let bottom = self
                .panels
                .iter()
                .filter(|r| r.overlaps_horizontally(probe, self.gap))
                .map(|r| r.max.y + self.gap)
                .fold(self.margin, f32::max);
            (self.top() - bottom).max(0.0)
        } else {
            let top = self
                .panels
                .iter()
                .filter(|r| r.overlaps_horizontally(probe, self.gap))
                .map(|r| r.min.y - self.gap)
                .fold(self.top(), f32::min);
            (top - self.margin).max(0.0)
        }
    }

    /// Find a non-overlapping place for an arbitrary measured panel. Returns
    /// `None` only when the panel cannot fit inside the screen's safe area.
    pub fn place(&mut self, size: Vec2, zone: Zone) -> Option<Rect> {
        let size = size.round();
        if size.x > self.screen.x - 2.0 * self.margin || size.y > self.top() - self.margin {
            return None;
        }
        if zone == Zone::BottomCenter {
            return self.place_bottom_center(size);
        }
        let mut x = if zone.left() {
            self.margin
        } else {
            self.screen.x - self.margin - size.x
        };
        // Every vertical move passes a panel and every wrap passes a column.
        for _ in 0..(self.panels.len() + 1).pow(2) + 1 {
            let mut y = if zone.bottom() {
                self.margin
            } else {
                self.top() - size.y
            };
            loop {
                let candidate = Rect {
                    min: Vec2::new(x, y),
                    max: Vec2::new(x + size.x, y + size.y),
                };
                if candidate.min.y < self.margin || candidate.max.y > self.top() {
                    break;
                }
                let Some(blocker) = self
                    .panels
                    .iter()
                    .copied()
                    .find(|r| candidate.overlaps(*r, self.gap))
                else {
                    self.panels.push(candidate);
                    return Some(candidate);
                };
                y = if zone.bottom() {
                    blocker.max.y + self.gap
                } else {
                    blocker.min.y - self.gap - size.y
                };
            }
            let column = Rect {
                min: Vec2::new(x, self.margin),
                max: Vec2::new(x + size.x, self.top()),
            };
            let blockers: Vec<_> = self
                .panels
                .iter()
                .filter(|r| column.overlaps_horizontally(**r, self.gap))
                .collect();
            if blockers.is_empty() {
                return None;
            }
            x = if zone.left() {
                blockers.iter().map(|r| r.max.x).fold(x, f32::max) + self.gap
            } else {
                blockers.iter().map(|r| r.min.x).fold(x + size.x, f32::min) - self.gap - size.x
            };
            if x < self.margin || x + size.x > self.screen.x - self.margin {
                return None;
            }
        }
        None
    }

    /// `Zone::BottomCenter`: the free spot in the lowest row that has one,
    /// as near the middle of the screen as it can be: centered if nothing's
    /// there, or else just beside whatever is in the way.
    fn place_bottom_center(&mut self, size: Vec2) -> Option<Rect> {
        let middle = ((self.screen.x - size.x) / 2.0).round();
        let (lowest, highest) = (self.margin, self.screen.x - self.margin - size.x);
        let mut y = self.margin;
        while y + size.y <= self.top() {
            let mut xs = vec![middle];
            for r in &self.panels {
                xs.push(r.max.x + self.gap);
                xs.push(r.min.x - self.gap - size.x);
            }
            xs.retain(|x| (lowest..=highest).contains(x));
            xs.sort_by(|a, b| (a - middle).abs().total_cmp(&(b - middle).abs()));
            let row = |x: f32| Rect {
                min: Vec2::new(x, y),
                max: Vec2::new(x + size.x, y + size.y),
            };
            if let Some(spot) = xs
                .into_iter()
                .map(row)
                .find(|c| !self.panels.iter().any(|r| c.overlaps(*r, self.gap)))
            {
                self.panels.push(spot);
                return Some(spot);
            }
            // Nothing free in this row: try just above the lowest panel in it.
            let band = Rect {
                min: Vec2::new(self.margin, y),
                max: Vec2::new(self.screen.x - self.margin, y + size.y),
            };
            y = self
                .panels
                .iter()
                .filter(|r| r.overlaps(band, 0.0))
                .map(|r| r.max.y + self.gap)
                .fold(f32::INFINITY, f32::min);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bottom_center_panel_centers_then_steps_aside_then_up() {
        let screen = Vec2::new(1600.0, 900.0);
        // Alone, it sits in the middle of the bottom edge.
        let mut dock = Dock::new(screen, 16.0, 8.0, 60.0);
        let strip = dock
            .place(Vec2::new(300.0, 90.0), Zone::BottomCenter)
            .unwrap();
        assert_eq!(strip.min, Vec2::new(650.0, 16.0));
        // Beside a wide tray at the bottom-left, it moves over just enough.
        let mut dock = Dock::new(screen, 16.0, 8.0, 60.0);
        let tray = dock
            .place(Vec2::new(800.0, 200.0), Zone::BottomLeft)
            .unwrap();
        let strip = dock
            .place(Vec2::new(300.0, 90.0), Zone::BottomCenter)
            .unwrap();
        assert_eq!(strip.min, Vec2::new(tray.max.x + 8.0, 16.0));
        // With no room beside it, it goes above.
        let mut dock = Dock::new(screen, 16.0, 8.0, 60.0);
        let tray = dock
            .place(Vec2::new(1400.0, 200.0), Zone::BottomLeft)
            .unwrap();
        let strip = dock
            .place(Vec2::new(300.0, 90.0), Zone::BottomCenter)
            .unwrap();
        assert_eq!(strip.min.y, tray.max.y + 8.0);
        assert!(!strip.overlaps(tray, 0.0));
    }

    #[test]
    fn panels_stack_and_wrap_without_intersecting() {
        let mut dock = Dock::new(Vec2::new(900.0, 700.0), 20.0, 10.0, 60.0);
        let tray = dock
            .place(Vec2::new(300.0, 350.0), Zone::BottomLeft)
            .unwrap();
        assert_eq!(tray.min, Vec2::new(20.0, 20.0));
        assert_eq!(dock.remaining_height(Zone::BottomLeft, 240.0), 260.0);
        let queue = dock
            .place(Vec2::new(240.0, 240.0), Zone::BottomLeft)
            .unwrap();
        assert_eq!(queue.min, Vec2::new(20.0, 380.0));
        let debug = dock.place(Vec2::new(260.0, 180.0), Zone::TopLeft).unwrap();
        assert!(debug.min.x >= tray.max.x + 10.0);
        assert!(!debug.overlaps(tray, 0.0));
        assert!(!debug.overlaps(queue, 0.0));
        let hover = dock.place(Vec2::new(180.0, 130.0), Zone::TopRight).unwrap();
        assert!(!hover.overlaps(debug, 0.0));
        let footer = dock
            .place(Vec2::new(150.0, 90.0), Zone::BottomRight)
            .unwrap();
        for other in [tray, queue, debug, hover] {
            assert!(!footer.overlaps(other, 0.0));
        }
    }

    #[test]
    fn oversized_panels_report_no_fit() {
        let mut dock = Dock::new(Vec2::new(500.0, 400.0), 20.0, 10.0, 50.0);
        assert!(
            dock.place(Vec2::new(470.0, 100.0), Zone::BottomLeft)
                .is_none()
        );
        assert!(
            dock.place(Vec2::new(100.0, 340.0), Zone::TopRight)
                .is_none()
        );
    }
}
