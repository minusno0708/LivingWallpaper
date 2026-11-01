use crate::{settings::Settings, simulation::Obstacle};
use glam::Vec2;
use std::sync::{mpsc::Sender, Arc, Mutex};
#[derive(Clone, Debug)]
pub enum Command {
    Mouse,
    Icons,
    Power,
    Mode(crate::settings::ModelMode),
    Count(usize),
    Fps(u32),
    TimeScale(f32),
    Monitor(String),
    Quit,
}
#[derive(Default)]
pub struct Environment {
    pub cursor: Option<Vec2>,
    pub battery: bool,
    pub suspended: bool,
    pub fullscreen: bool,
}
#[cfg(windows)]
#[path = "windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::*;
#[cfg(not(windows))]
pub fn environment(_: isize) -> Environment {
    Environment::default()
}
#[cfg(not(windows))]
pub fn icons(_: isize) -> anyhow::Result<Vec<Obstacle>> {
    Ok(vec![])
}
#[cfg(not(windows))]
pub fn attach(_: isize) -> anyhow::Result<()> {
    anyhow::bail!("Wallpaper integration is Windows-only; use --preview")
}
#[cfg(not(windows))]
pub fn start_tray(_: Sender<Command>, _: Arc<Mutex<Settings>>) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn healthy(_: isize) -> bool {
    true
}
#[cfg(not(windows))]
pub fn single_instance() -> anyhow::Result<()> {
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[cfg(not(windows))]
pub fn displays() -> Vec<Display> {
    vec![]
}
#[cfg(not(windows))]
pub fn configure_displays(_: isize, _: &[String]) -> anyhow::Result<Vec<Obstacle>> {
    Ok(vec![])
}

/// Solid cells outside enabled displays, in the shared wallpaper coordinate space.
pub fn display_gaps(displays: &[Display], disabled: &[String]) -> Vec<Obstacle> {
    if displays.is_empty() {
        return vec![];
    }
    let left = displays.iter().map(|d| d.x).min().unwrap();
    let top = displays.iter().map(|d| d.y).min().unwrap();
    let right = displays.iter().map(|d| d.x + d.width).max().unwrap();
    let bottom = displays.iter().map(|d| d.y + d.height).max().unwrap();
    let active: Vec<_> = displays
        .iter()
        .filter(|d| !disabled.contains(&d.id))
        .collect();
    // Partition the complement of the active monitor union into solid cells.
    let mut xs = vec![left, right];
    let mut ys = vec![top, bottom];
    for d in &active {
        xs.extend([d.x, d.x + d.width]);
        ys.extend([d.y, d.y + d.height]);
    }
    xs.sort();
    xs.dedup();
    ys.sort();
    ys.dedup();
    let mut gaps = Vec::new();
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            if !active.iter().any(|d| {
                x[0] >= d.x && x[1] <= d.x + d.width && y[0] >= d.y && y[1] <= d.y + d.height
            }) {
                gaps.push(Obstacle {
                    min: Vec2::new((x[0] - left) as f32, (y[0] - top) as f32),
                    max: Vec2::new((x[1] - left) as f32, (y[1] - top) as f32),
                });
            }
        }
    }
    gaps
}
#[cfg(test)]
mod display_tests {
    use super::*;
    fn display(id: &str, x: i32, y: i32) -> Display {
        Display {
            id: id.into(),
            name: id.into(),
            x,
            y,
            width: 1920,
            height: 1080,
        }
    }
    #[test]
    fn adjacent_negative_origin_displays_have_no_internal_barrier() {
        assert!(display_gaps(&[display("left", -1920, 0), display("main", 0, 0)], &[]).is_empty());
    }
    #[test]
    fn disabled_display_and_layout_holes_are_solid() {
        let displays = [display("left", -1920, 0), display("main", 0, 0)];
        let gaps = display_gaps(&displays, &["left".into()]);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].min, Vec2::ZERO);
        assert_eq!(gaps[0].max, Vec2::new(1920.0, 1080.0));
        let diagonal = [display("left", -1920, -1080), display("main", 0, 0)];
        let gaps = display_gaps(&diagonal, &[]);
        assert_eq!(gaps.len(), 2);
        assert_eq!(
            gaps.iter()
                .map(|g| (g.max - g.min).x * (g.max - g.min).y)
                .sum::<f32>(),
            2.0 * 1920.0 * 1080.0
        );
    }
    #[test]
    fn settings_default_to_all_displays_and_persist_exclusions() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.disabled_displays.is_empty());
        settings.disabled_displays.push("display-test".into());
        let loaded: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(loaded.disabled_displays, settings.disabled_displays);
    }
}
