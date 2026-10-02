use serde::{Deserialize, Serialize};
use std::path::Path;

/// Application configuration loaded from `config.yaml` at startup.
/// If the file does not exist, a default is written so the user can discover it.
/// Missing fields fall back to their defaults, unknown fields are ignored.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub window_width: f32,
    pub window_height: f32,
    /// The level yaml to load (see `docs/LEVELS.md`). If the file doesn't
    /// exist, the game writes an editable template there on first run.
    pub level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: 1280.0,
            window_height: 960.0,
            level: "levels/valley.yaml".to_string(),
        }
    }
}

const CONFIG_PATH: &str = "config.yaml";

impl AppConfig {
    /// Load config from `config.yaml`, falling back to `Default` on any error.
    /// Writes the default file if it does not yet exist.
    pub fn load() -> Self {
        let path = Path::new(CONFIG_PATH);
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(contents) => match serde_yaml::from_str::<AppConfig>(&contents) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Failed to parse {CONFIG_PATH}: {e}, using defaults");
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read {CONFIG_PATH}: {e}, using defaults");
                }
            }
        } else {
            // Write default config so the user discovers it
            let config = AppConfig::default();
            if let Ok(yaml) = serde_yaml::to_string(&config) {
                if let Err(e) = std::fs::write(path, yaml) {
                    eprintln!("Failed to write default {CONFIG_PATH}: {e}");
                } else {
                    println!("Wrote default config to {CONFIG_PATH}");
                }
            }
        }
        AppConfig::default()
    }
}
