//! Level loading — terrain and starting layout defined OUTSIDE the code.
//!
//! A level is two files that live next to each other:
//!   * `<name>.yaml` — metadata: which heightmap to use, how tall it is, where
//!     the water source sits, and any starting objects.
//!   * `<name>.png`  — a grayscale heightmap. Black = lowest ground, white =
//!     highest. Any image editor is the level editor: paint a darker line and
//!     the water will find it.
//!
//! The PNG can be any size; it is bilinearly resampled onto the game's W×D
//! grid at load. Positions in the yaml are fractions of the map (0.0–1.0) so a
//! level is independent of both image size and grid resolution.
//!
//! `write_template` does the reverse — it bakes a heights array out to a
//! yaml + 16-bit PNG pair, which is how the built-in valley terrain becomes an
//! editable starting point on first run.

use serde::{Deserialize, Serialize};
use std::path::Path;

type Gray16 = image::ImageBuffer<image::Luma<u16>, Vec<u16>>;

/// A level as written in its `.yaml` file.
#[derive(Serialize, Deserialize, Clone)]
pub struct LevelConfig {
    pub name: String,
    /// Grayscale PNG path, relative to the yaml file. Black = low, white = high.
    pub heightmap: String,
    /// World height of a pure-white pixel (black is 0).
    pub height_scale: f32,
    pub source: SourceConfig,
    #[serde(default)]
    pub objects: Vec<ObjectConfig>,
}

/// Where water enters the map. `x`/`z` are fractions of the map: x runs
/// 0 (left) → 1 (right), z runs 0 (back) → 1 (front — the front edge drains).
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct SourceConfig {
    pub x: f32,
    pub z: f32,
    /// Patch radius in cells (wider = gentler inflow, no spike).
    pub radius: i32,
    /// Water depth added per second, spread over the patch.
    pub rate: f32,
}

/// A starting object: map-fraction position + weight in kg.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct ObjectConfig {
    pub x: f32,
    pub z: f32,
    pub weight: f32,
}

/// A level resolved against the runtime grid: heights resampled to `w × d`.
pub struct LoadedLevel {
    pub name: String,
    pub heights: Vec<f32>,
    pub source: SourceConfig,
    pub objects: Vec<ObjectConfig>,
}

/// Load a level yaml + its heightmap PNG, resampling the image to `w × d`.
pub fn load(path: &str, w: usize, d: usize) -> Result<LoadedLevel, String> {
    let yaml = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let config: LevelConfig =
        serde_yaml::from_str(&yaml).map_err(|e| format!("parse {path}: {e}"))?;
    let dir = Path::new(path).parent().unwrap_or(Path::new("."));
    let img_path = dir.join(&config.heightmap);
    let img = image::open(&img_path)
        .map_err(|e| format!("open heightmap {}: {e}", img_path.display()))?
        .to_luma16(); // 8-bit PNGs are widened automatically, so both work
    let heights = resample(&img, w, d, config.height_scale);
    Ok(LoadedLevel { name: config.name, heights, source: config.source, objects: config.objects })
}

/// Bilinearly sample the heightmap onto the grid; white (65535) maps to `scale`.
fn resample(img: &Gray16, w: usize, d: usize, scale: f32) -> Vec<f32> {
    let (iw, ih) = (img.width() as usize, img.height() as usize);
    let px = |x: usize, z: usize| img.get_pixel(x as u32, z as u32).0[0] as f32 / 65535.0;
    let mut heights = vec![0.0; w * d];
    for z in 0..d {
        for x in 0..w {
            // Grid position as a 0..1 fraction, then in image-pixel space.
            let fx = if w > 1 { x as f32 / (w - 1) as f32 } else { 0.0 } * (iw - 1) as f32;
            let fz = if d > 1 { z as f32 / (d - 1) as f32 } else { 0.0 } * (ih - 1) as f32;
            let (x0, z0) = (fx.floor() as usize, fz.floor() as usize);
            let (x1, z1) = ((x0 + 1).min(iw - 1), (z0 + 1).min(ih - 1));
            let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
            let back = px(x0, z0) * (1.0 - tx) + px(x1, z0) * tx;
            let front = px(x0, z1) * (1.0 - tx) + px(x1, z1) * tx;
            heights[z * w + x] = (back * (1.0 - tz) + front * tz) * scale;
        }
    }
    heights
}

/// Write `level` out as an editable yaml + 16-bit grayscale PNG pair at `path`
/// (the PNG lands next to the yaml with the same stem). Heights are normalised
/// so the tallest point is pure white and `height_scale` records the real max.
pub fn write_template(path: &str, level: &LoadedLevel, w: usize, d: usize) -> Result<(), String> {
    let yaml_path = Path::new(path);
    let dir = yaml_path.parent().unwrap_or(Path::new("."));
    let stem = yaml_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("bad level path: {path}"))?;
    let png_name = format!("{stem}.png");

    let max = level.heights.iter().fold(0.0f32, |a, &b| a.max(b)).max(1.0);
    let mut img = Gray16::new(w as u32, d as u32);
    for z in 0..d {
        for x in 0..w {
            let t = (level.heights[z * w + x] / max).clamp(0.0, 1.0);
            img.put_pixel(x as u32, z as u32, image::Luma([(t * 65535.0).round() as u16]));
        }
    }

    if !dir.as_os_str().is_empty() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let png_path = dir.join(&png_name);
    img.save(&png_path).map_err(|e| format!("write {}: {e}", png_path.display()))?;

    let config = LevelConfig {
        name: level.name.clone(),
        heightmap: png_name,
        height_scale: max,
        source: level.source,
        objects: level.objects.clone(),
    };
    let yaml = serde_yaml::to_string(&config).map_err(|e| format!("serialise level: {e}"))?;
    std::fs::write(yaml_path, yaml).map_err(|e| format!("write {path}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same-size image, corner pixels: values map straight through the scale.
    #[test]
    fn test_resample_identity_corners() {
        let mut img = Gray16::new(2, 2);
        img.put_pixel(0, 0, image::Luma([0]));
        img.put_pixel(1, 0, image::Luma([65535]));
        img.put_pixel(0, 1, image::Luma([65535 / 2]));
        img.put_pixel(1, 1, image::Luma([65535]));
        let h = resample(&img, 2, 2, 100.0);
        assert!((h[0] - 0.0).abs() < 0.01);
        assert!((h[1] - 100.0).abs() < 0.01);
        assert!((h[2] - 50.0).abs() < 0.01); // half-gray ≈ half the scale
        assert!((h[3] - 100.0).abs() < 0.01);
    }

    /// Upsampling a 2×1 black→white gradient: the middle sample is the average.
    #[test]
    fn test_resample_bilinear_midpoint() {
        let mut img = Gray16::new(2, 1);
        img.put_pixel(0, 0, image::Luma([0]));
        img.put_pixel(1, 0, image::Luma([65535]));
        let h = resample(&img, 3, 1, 10.0);
        assert!((h[0] - 0.0).abs() < 0.01);
        assert!((h[1] - 5.0).abs() < 0.01);
        assert!((h[2] - 10.0).abs() < 0.01);
    }

    /// write_template → load gives back the same heights (within quantisation).
    #[test]
    fn test_template_round_trip() {
        let dir = std::env::temp_dir().join("bluerush_level_test");
        let path = dir.join("round-trip.yaml");
        let (w, d) = (8, 8);
        let heights: Vec<f32> = (0..w * d).map(|i| (i % w) as f32 * 3.0).collect();
        let level = LoadedLevel {
            name: "Round Trip".into(),
            heights: heights.clone(),
            source: SourceConfig { x: 0.5, z: 0.1, radius: 3, rate: 50.0 },
            objects: vec![ObjectConfig { x: 0.25, z: 0.75, weight: 800.0 }],
        };
        write_template(path.to_str().unwrap(), &level, w, d).unwrap();
        let loaded = load(path.to_str().unwrap(), w, d).unwrap();
        assert_eq!(loaded.name, "Round Trip");
        assert_eq!(loaded.objects.len(), 1);
        assert!((loaded.source.rate - 50.0).abs() < 0.01);
        for (a, b) in heights.iter().zip(&loaded.heights) {
            assert!((a - b).abs() < 0.01, "height mismatch: {a} vs {b}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
