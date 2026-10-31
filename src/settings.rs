use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub fish_count: usize,
    pub fps: u32,
    pub mouse: bool,
    pub icons: bool,
    pub power_saving: bool,
    pub paused: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            fish_count: 240,
            fps: 30,
            mouse: true,
            icons: true,
            power_saving: true,
            paused: false,
        }
    }
}
impl Settings {
    pub fn normalize(&mut self) {
        self.fish_count = self.fish_count.clamp(32, 1000);
        self.fps = self.fps.clamp(10, 60);
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
            .and_then(|b| serde_json::from_slice(&b).ok())
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
        assert_eq!((s.fish_count, s.fps, s.mouse), (1000, 10, true));
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
        std::fs::remove_file(current).unwrap();
        std::fs::remove_file(legacy).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
