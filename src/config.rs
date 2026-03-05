use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub window_width: f32,
    pub window_height: f32,
    pub grid_cols: usize,
    pub grid_rows: usize,
    pub tile_size: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: 800.0,
            window_height: 600.0,
            grid_cols: 42,
            grid_rows: 37,
            tile_size: 16.0,
        }
    }
}

const CONFIG_PATH: &str = "config.yaml";

impl AppConfig {
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
