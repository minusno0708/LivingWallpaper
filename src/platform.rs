use crate::{settings::Settings, simulation::Obstacle};
use glam::Vec2;
use std::sync::{mpsc::Sender, Arc, Mutex};
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Pause,
    Mouse,
    Icons,
    Power,
    Count(usize),
    Fps(u32),
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
