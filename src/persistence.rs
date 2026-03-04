use serde::{Deserialize, Serialize};
use std::sync::{mpsc, Mutex};

use crate::simulation::Cell;
use crate::simulation::Grid;

#[derive(Serialize, Deserialize)]
struct SaveData {
    tile_size: f32,
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

/// Channel-based result receiver so file dialogs run off the main thread.
/// Wrapped in Mutex to satisfy Bevy's Sync requirement on resources.
pub enum PendingIo {
    Save(Mutex<mpsc::Receiver<Result<(), String>>>),
    Load(Mutex<mpsc::Receiver<Result<Vec<Cell>, String>>>),
}

/// Kick off a save on a background thread. Returns a receiver to poll for the result.
pub fn save_grid_async(grid: &Grid, tile_size: f32) -> PendingIo {
    let data = SaveData {
        tile_size,
        width: grid.width,
        height: grid.height,
        cells: grid.cells.clone(),
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        tx.send(do_save(data)).ok();
    });
    PendingIo::Save(Mutex::new(rx))
}

fn do_save(data: SaveData) -> Result<(), String> {
    let path = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .save_file();

    let Some(path) = path else {
        return Ok(()); // user cancelled
    };

    let json = serde_json::to_string_pretty(&data).map_err(|e| format!("Serialize error: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {e}"))?;
    println!("Grid saved to {}", path.display());
    Ok(())
}

/// Kick off a load on a background thread. Returns a receiver to poll for the result.
pub fn load_grid_async(
    current_tile_size: f32,
    current_width: usize,
    current_height: usize,
) -> PendingIo {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        tx.send(do_load(current_tile_size, current_width, current_height)).ok();
    });
    PendingIo::Load(Mutex::new(rx))
}

fn do_load(
    current_tile_size: f32,
    current_width: usize,
    current_height: usize,
) -> Result<Vec<Cell>, String> {
    let path = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .pick_file();

    let Some(path) = path else {
        return Err("Cancelled".into());
    };

    let json = std::fs::read_to_string(&path).map_err(|e| format!("Read error: {e}"))?;
    let data: SaveData =
        serde_json::from_str(&json).map_err(|e| format!("Deserialize error: {e}"))?;

    if (data.tile_size - current_tile_size).abs() > 0.01 {
        return Err(format!(
            "Tile size mismatch: file has {}, current is {}",
            data.tile_size, current_tile_size
        ));
    }
    if data.width != current_width || data.height != current_height {
        return Err(format!(
            "Grid size mismatch: file is {}x{}, current is {}x{}",
            data.width, data.height, current_width, current_height
        ));
    }

    println!("Grid loaded from {}", path.display());
    Ok(data.cells)
}
