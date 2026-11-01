use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FPS_CHOICES: [u32; 9] = [0, 15, 30, 45, 60, 75, 90, 105, 120];
pub fn normalize_time_scale(value: f32) -> f32 {
    if value.is_finite() {
        (value.clamp(0.0, 2.0) * 4.0).round() / 4.0
    } else {
        1.0
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMode {
    #[default]
    Fish,
    Eyes,
}
impl ModelMode {
    pub const ALL: [Self; 2] = [Self::Fish, Self::Eyes];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fish => "Fish",
            Self::Eyes => "Eyes",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub disabled_displays: Vec<String>,
    pub mode: ModelMode,
    pub fish_count: usize,
    pub fps: u32,
    pub time_scale: f32,
    pub mouse: bool,
    pub icons: bool,
    pub power_saving: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            disabled_displays: vec![],
            mode: ModelMode::Fish,
            fish_count: 240,
            fps: 30,
            time_scale: 1.0,
            mouse: true,
            icons: true,
            power_saving: true,
        }
    }
}
impl Settings {
    pub fn normalize(&mut self) {
        self.time_scale = normalize_time_scale(self.time_scale);
        self.fish_count = ((self.fish_count.clamp(10, 1000) + 5) / 10) * 10;
        self.fps = *FPS_CHOICES
            .iter()
            .min_by_key(|&&n| n.abs_diff(self.fps))
            .unwrap();
    }
    fn path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("LivingWallpaper")
            .join("settings.json")
    }
    pub fn load() -> Self {
        let path = Self::path();
        let legacy = path
            .parent()
            .unwrap()
            .with_file_name("FishSchoolWallpaper")
            .join("settings.json");
        Self::load_from(&path, &legacy)
    }
    fn load_from(path: &Path, legacy: &Path) -> Self {
        let mut settings: Self = std::fs::read(path)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    std::fs::read(legacy)
                } else {
                    Err(error)
                }
            })
            .ok()
            .and_then(|b| {
                let value: serde_json::Value = serde_json::from_slice(&b).ok()?;
                let was_paused = value
                    .get("paused")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let mut settings: Self = serde_json::from_value(value).ok()?;
                // Consume the removed legacy pause flag once; FPS now owns stop/resume.
                if was_paused {
                    settings.fps = 0;
                }
                Some(settings)
            })
            .unwrap_or_default();
        settings.normalize();
        settings
    }
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_settings_are_bounded() {
        let mut s: Settings = serde_json::from_str(r#"{"fish_count":999999,"fps":0}"#).unwrap();
        s.normalize();
        assert_eq!((s.fish_count, s.fps, s.mouse), (1000, 0, true));
    }
    #[test]
    fn slider_steps_and_legacy_mode_are_normalized() {
        let mut s: Settings = serde_json::from_str(r#"{"fish_count":333,"fps":24}"#).unwrap();
        s.normalize();
        assert_eq!((s.fish_count, s.fps, s.mode), (330, 30, ModelMode::Fish));
        assert_eq!(s.time_scale, 1.0);
        s.fish_count = 0;
        s.fps = 120;
        s.normalize();
        assert_eq!((s.fish_count, s.fps), (10, 120));
        for fps in FPS_CHOICES {
            s.fps = fps;
            s.normalize();
            assert_eq!(s.fps, fps);
        }
    }
    #[test]
    fn time_scale_is_bounded_quantized_and_persisted() {
        for (input, expected) in [
            (-1.0, 0.0),
            (0.0, 0.0),
            (0.25, 0.25),
            (1.3, 1.25),
            (4.0, 2.0),
            (f32::NAN, 1.0),
            (f32::INFINITY, 1.0),
        ] {
            let mut s = Settings {
                time_scale: input,
                ..Settings::default()
            };
            s.normalize();
            assert_eq!(s.time_scale, expected);
            let saved = serde_json::to_vec(&s).unwrap();
            let loaded: Settings = serde_json::from_slice(&saved).unwrap();
            assert_eq!(loaded.time_scale, expected);
        }
    }
    #[test]
    fn renamed_app_loads_legacy_settings_but_prefers_current() {
        let root = std::env::temp_dir().join(format!(
            "swarm-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let current = root.join("current.json");
        let legacy = root.join("legacy.json");
        std::fs::write(&legacy, r#"{"fish_count":120,"mouse":false}"#).unwrap();
        let loaded = Settings::load_from(&current, &legacy);
        assert_eq!(loaded.fish_count, 120);
        assert!(!loaded.mouse);
        std::fs::write(&current, r#"{"fish_count":500,"mouse":true}"#).unwrap();
        let loaded = Settings::load_from(&current, &legacy);
        assert_eq!(loaded.fish_count, 500);
        assert!(loaded.mouse);
        std::fs::write(&current, r#"{"fps":60,"paused":true}"#).unwrap();
        let migrated = Settings::load_from(&current, &legacy);
        assert_eq!(migrated.fps, 0);
        let saved = serde_json::to_value(&migrated).unwrap();
        assert!(
            saved.get("paused").is_none(),
            "removed pause flag must not be saved"
        );
        std::fs::remove_file(current).unwrap();
        std::fs::remove_file(legacy).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

#[cfg(test)]
mod eyes_settings_tests {
    use super::*;
    #[test]
    fn eyes_mode_round_trips_without_changing_fish_defaults() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.mode, ModelMode::Fish);
        let settings = Settings {
            mode: ModelMode::Eyes,
            ..Settings::default()
        };
        let loaded: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(loaded.mode, ModelMode::Eyes);
        assert_eq!(loaded.fish_count, 240);
    }
}
