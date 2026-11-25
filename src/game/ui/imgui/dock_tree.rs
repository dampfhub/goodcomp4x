//! How panels dock: beside each other (`QueueDockRelation`), and in the
//! game window (`DockTree`), as plain data the layout keeps and saves.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct QueueDockRelation {
    // Queue's position relative to Selection in their shared dock node,
    // and its share of their room (`split_ratio` makes it ImGui's ratio).
    pub(super) direction: ::imgui::sys::ImGuiDir,
    pub(super) fraction: f32,
    pub(super) anchor_group: bool,
}
/// Where a panel is docked in the game window (#333): the side of the map
/// (the dockspace's central node) its part of the dockspace is on, and how
/// wide (left, right) or tall (up, down) that part is, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EdgeDock {
    pub(super) side: ::imgui::sys::ImGuiDir,
    pub(super) extent: f32,
}

impl EdgeDock {
    /// The same place, give or take the pixels a split rounds off.
    pub(super) fn matches(a: Option<Self>, b: Option<Self>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => a.side == b.side && (a.extent - b.extent).abs() < 3.0,
            (a, b) => a.is_none() && b.is_none(),
        }
    }

    pub(super) fn side_text(self) -> &'static str {
        match self.side {
            ::imgui::sys::ImGuiDir_Left => "left",
            ::imgui::sys::ImGuiDir_Right => "right",
            ::imgui::sys::ImGuiDir_Up => "up",
            _ => "down",
        }
    }

    /// Read from a layout saved before each view kept its docking (#360),
    /// when only Debug's was kept by view (`debug_edge` lines).
    pub(super) fn from_text(side: &str, extent: &str) -> Option<Self> {
        let side = match side {
            "left" => ::imgui::sys::ImGuiDir_Left,
            "right" => ::imgui::sys::ImGuiDir_Right,
            "up" => ::imgui::sys::ImGuiDir_Up,
            "down" => ::imgui::sys::ImGuiDir_Down,
            _ => return None,
        };
        let extent = extent
            .parse::<f32>()
            .ok()
            .filter(|e| e.is_finite() && *e > 0.0)?;
        Some(Self { side, extent })
    }
}

/// How the panels are docked in the game window (#333, #360): the tree of
/// its dockspace's nodes. Each view's layout keeps one (`view_docks`), and
/// switching views builds that view's (`build_game_dock`).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum DockTree {
    /// The map: the dockspace's central node, which takes no panel.
    Map,
    /// Panels docked together, as tabs, by slot.
    Panels(Vec<usize>),
    /// Two parts, side by side or (`vertical`) one over the other, and how
    /// wide or tall each is in pixels, but for the part holding the map,
    /// which has 0: it takes whatever room the other leaves.
    Split {
        vertical: bool,
        sizes: [f32; 2],
        parts: Box<[DockTree; 2]>,
    },
}

impl DockTree {
    pub(super) fn has_map(&self) -> bool {
        self.maps() > 0
    }

    fn maps(&self) -> usize {
        match self {
            Self::Map => 1,
            Self::Panels(_) => 0,
            Self::Split { parts, .. } => parts[0].maps() + parts[1].maps(),
        }
    }

    /// The panels docked, by slot.
    pub(super) fn slots(&self) -> Vec<usize> {
        match self {
            Self::Map => Vec::new(),
            Self::Panels(slots) => slots.clone(),
            Self::Split { parts, .. } => {
                let mut slots = parts[0].slots();
                slots.extend(parts[1].slots());
                slots
            }
        }
    }

    pub(super) fn contains(&self, slot: usize) -> bool {
        self.slots().contains(&slot)
    }

    /// The same without `slot`: a part it leaves empty goes, and its split
    /// with it. None if nothing is left.
    pub(super) fn without(&self, slot: usize) -> Option<Self> {
        match self {
            Self::Map => Some(Self::Map),
            Self::Panels(slots) => {
                let left: Vec<_> = slots.iter().copied().filter(|&s| s != slot).collect();
                (!left.is_empty()).then_some(Self::Panels(left))
            }
            Self::Split {
                vertical,
                sizes,
                parts,
            } => match (parts[0].without(slot), parts[1].without(slot)) {
                (Some(a), Some(b)) => Some(Self::Split {
                    vertical: *vertical,
                    sizes: *sizes,
                    parts: Box::new([a, b]),
                }),
                (a, b) => a.or(b),
            },
        }
    }

    /// Whether the two dock the same panels the same way, give or take the
    /// pixels a split rounds off.
    pub(super) fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Map, Self::Map) => true,
            (Self::Panels(a), Self::Panels(b)) => {
                let (mut a, mut b) = (a.clone(), b.clone());
                a.sort_unstable();
                b.sort_unstable();
                a == b
            }
            (
                Self::Split {
                    vertical,
                    sizes,
                    parts,
                },
                Self::Split {
                    vertical: other_vertical,
                    sizes: other_sizes,
                    parts: other_parts,
                },
            ) => {
                vertical == other_vertical
                    && sizes
                        .iter()
                        .zip(other_sizes)
                        .all(|(a, b)| (a - b).abs() < 3.0)
                    && parts[0].matches(&other_parts[0])
                    && parts[1].matches(&other_parts[1])
            }
            _ => false,
        }
    }

    /// Which side of the map `slot`'s part of the game window is on, and
    /// how wide or tall that part is, if it's docked there.
    pub(super) fn edge_of(&self, slot: usize) -> Option<EdgeDock> {
        let Self::Split {
            vertical,
            sizes,
            parts,
        } = self
        else {
            return None;
        };
        let i = parts.iter().position(|part| part.contains(slot))?;
        if !parts[1 - i].has_map() {
            return parts[i].edge_of(slot);
        }
        let side = match (vertical, i) {
            (false, 0) => ::imgui::sys::ImGuiDir_Left,
            (false, _) => ::imgui::sys::ImGuiDir_Right,
            (true, 0) => ::imgui::sys::ImGuiDir_Up,
            (true, _) => ::imgui::sys::ImGuiDir_Down,
        };
        Some(EdgeDock {
            side,
            extent: sizes[i],
        })
    }

    /// The same with `slot` docked beside the map on `edge`'s side, as wide
    /// or tall as it says.
    pub(super) fn beside_map(self, slot: usize, edge: EdgeDock) -> Self {
        match self {
            Self::Map => {
                let panel = Self::Panels(vec![slot]);
                let (parts, sizes) = match edge.side {
                    ::imgui::sys::ImGuiDir_Left | ::imgui::sys::ImGuiDir_Up => {
                        ([panel, Self::Map], [edge.extent, 0.0])
                    }
                    _ => ([Self::Map, panel], [0.0, edge.extent]),
                };
                Self::Split {
                    vertical: matches!(
                        edge.side,
                        ::imgui::sys::ImGuiDir_Up | ::imgui::sys::ImGuiDir_Down
                    ),
                    sizes,
                    parts: Box::new(parts),
                }
            }
            Self::Split {
                vertical,
                sizes,
                parts,
            } => {
                let [a, b] = *parts;
                let parts = if a.has_map() {
                    [a.beside_map(slot, edge), b]
                } else {
                    [a, b.beside_map(slot, edge)]
                };
                Self::Split {
                    vertical,
                    sizes,
                    parts: Box::new(parts),
                }
            }
            panels => panels,
        }
    }

    /// As words for the saved layout, which `from_words` reads back.
    pub(super) fn words(&self, out: &mut Vec<String>) {
        match self {
            Self::Map => out.push("map".into()),
            Self::Panels(slots) => {
                out.push(format!("panels {}", slots.len()));
                out.extend(slots.iter().map(usize::to_string));
            }
            Self::Split {
                vertical,
                sizes,
                parts,
            } => {
                let axis = if *vertical { "y" } else { "x" };
                out.push(format!("split {axis} {} {}", sizes[0], sizes[1]));
                parts[0].words(out);
                parts[1].words(out);
            }
        }
    }

    /// Read back from `words`, if they make a docking the game window can
    /// have: the map once, and each panel no more than once.
    pub(super) fn from_words(words: &[&str]) -> Option<Self> {
        let mut words = words.iter().copied();
        let tree = Self::read(&mut words, 0)?;
        let mut slots = tree.slots();
        let count = slots.len();
        slots.sort_unstable();
        slots.dedup();
        (words.next().is_none() && tree.maps() == 1 && slots.len() == count).then_some(tree)
    }

    pub(super) fn read<'a>(
        words: &mut impl Iterator<Item = &'a str>,
        depth: usize,
    ) -> Option<Self> {
        // Deeper than any game window's panels could make it: damaged.
        if depth > 2 * SLOT_COUNT {
            return None;
        }
        match words.next()? {
            "map" => Some(Self::Map),
            "panels" => {
                let count = words.next()?.parse::<usize>().ok()?;
                if count == 0 || count > SLOT_COUNT {
                    return None;
                }
                let slots = (0..count)
                    .map(|_| {
                        words
                            .next()?
                            .parse::<usize>()
                            .ok()
                            .filter(|&s| s < SLOT_COUNT)
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(Self::Panels(slots))
            }
            "split" => {
                let vertical = match words.next()? {
                    "x" => false,
                    "y" => true,
                    _ => return None,
                };
                let mut size = || {
                    words
                        .next()?
                        .parse::<f32>()
                        .ok()
                        .filter(|s| s.is_finite() && *s >= 0.0)
                };
                let sizes = [size()?, size()?];
                let a = Self::read(words, depth + 1)?;
                let b = Self::read(words, depth + 1)?;
                Some(Self::Split {
                    vertical,
                    sizes,
                    parts: Box::new([a, b]),
                })
            }
            _ => None,
        }
    }
}
/// The ratio ImGui's dock requests take for docking a window at
/// `direction` with `share` of the room: they give it to the split's first
/// part, left or up (`DockNodeTreeSplit`), which the docked window is only
/// when it goes left or up (#356).
pub(super) fn split_ratio(direction: ::imgui::sys::ImGuiDir, share: f32) -> f32 {
    match direction {
        ::imgui::sys::ImGuiDir_Left | ::imgui::sys::ImGuiDir_Up => share,
        _ => 1.0 - share,
    }
}

pub(super) fn opposite_dock_direction(direction: ::imgui::sys::ImGuiDir) -> ::imgui::sys::ImGuiDir {
    match direction {
        ::imgui::sys::ImGuiDir_Left => ::imgui::sys::ImGuiDir_Right,
        ::imgui::sys::ImGuiDir_Right => ::imgui::sys::ImGuiDir_Left,
        ::imgui::sys::ImGuiDir_Up => ::imgui::sys::ImGuiDir_Down,
        ::imgui::sys::ImGuiDir_Down => ::imgui::sys::ImGuiDir_Up,
        _ => ::imgui::sys::ImGuiDir_None,
    }
}
