use serde::{Deserialize, Serialize};
use std::path::Path;

/// Application configuration loaded from `config.yaml` at startup.
/// If the file does not exist, a default is written so the user can discover it.
#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub window_width: f32,
    pub window_height: f32,
    pub grid_cols: usize,
    pub grid_rows: usize,
    pub tile_size: f32,
    pub collision_destruction: bool,
    pub level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: 800.0,
            window_height: 600.0,
            grid_cols: 42,
            grid_rows: 37,
            tile_size: 16.0,
            collision_destruction: false,
            level: "levels/coastal-bowl.json".to_string(),
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
