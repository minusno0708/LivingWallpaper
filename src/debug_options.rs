//! Explicit opt-in debug growth; never persisted in normal settings.
use crate::settings::{ModelMode, Settings};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
pub const SPEEDS: [u32; 8] = [0, 1, 10, 60, 600, 3600, 21600, 86400];
static ENABLED: AtomicBool = AtomicBool::new(false);
static SPEED: AtomicU32 = AtomicU32::new(3600);
pub fn parse(args: &[String]) -> anyhow::Result<Option<u32>> {
    let mut selected = None;
    for (i, arg) in args.iter().enumerate() {
        if arg == "--plant-debug" {
            if selected.is_none() {
                selected = Some(3600);
            }
        }
        if arg == "--plant-debug-speed" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("--plant-debug-speed requires a multiplier"))?
                .parse::<u32>()?;
            anyhow::ensure!(
                SPEEDS.contains(&value),
                "Supported Plant debug speeds: 0, 1, 10, 60, 600, 3600, 21600, 86400"
            );
            selected = Some(value);
        }
    }
    Ok(selected)
}
pub fn configure(speed: Option<u32>) {
    ENABLED.store(speed.is_some(), Ordering::Relaxed);
    if let Some(speed) = speed {
        set_speed(speed);
    }
}
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub fn speed() -> u32 {
    SPEED.load(Ordering::Relaxed)
}
pub fn set_speed(value: u32) {
    if SPEEDS.contains(&value) {
        SPEED.store(value, Ordering::Relaxed);
    }
}
pub fn active(mode: ModelMode) -> bool {
    enabled() && mode == ModelMode::Plant
}
pub fn effective_scale(settings: &Settings) -> f64 {
    if active(settings.mode) {
        speed() as f64
    } else {
        settings.time_scale as f64
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn debug_is_opt_in_and_multipliers_are_bounded() {
        assert_eq!(parse(&args(&["app"])).unwrap(), None);
        assert_eq!(parse(&args(&["app", "--plant-debug"])).unwrap(), Some(3600));
        for value in SPEEDS {
            assert_eq!(
                parse(&args(&["app", "--plant-debug-speed", &value.to_string()])).unwrap(),
                Some(value)
            );
        }
        for bad in ["-1", "NaN", "1000000", "42"] {
            assert!(parse(&args(&["app", "--plant-debug-speed", bad])).is_err());
        }
        assert!(parse(&args(&["app", "--plant-debug-speed"])).is_err());
    }
}
