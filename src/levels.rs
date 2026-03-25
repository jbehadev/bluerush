use bevy::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;

use crate::grid::{GameState, GridConfig, InletMode};
use crate::simulation::{Cell, Grid};

/// Deserialised representation of a `levels/*.json` file.
#[derive(Deserialize)]
pub struct LevelData {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub default_inlet_mode: InletMode,
    pub cells: Vec<CellPlacement>,
}

/// A single non-Air cell placement within a level file.
#[derive(Deserialize)]
pub struct CellPlacement {
    pub x: usize,
    pub y: usize,
    pub cell: Cell,
}

/// Resource that tracks which level file is currently loaded.
#[derive(Resource)]
pub struct CurrentLevel {
    pub path: PathBuf,
}

/// Reads a level JSON file and populates the `Grid`, `GameState`, and `InletMode`.
/// On any error (file missing, malformed JSON), logs the error and falls back
/// to `Grid::init` using `GridConfig` dimensions.
pub fn load_level(
    path: &PathBuf,
    grid: &mut Grid,
    state: &mut GameState,
    inlet_mode: &mut InletMode,
    config: &GridConfig,
) {
    match try_load_level(path, grid, inlet_mode) {
        Ok(()) => {
            state.water_flow = false;
            state.gate_progress = 0;
        }
        Err(e) => {
            warn!("Failed to load level {:?}: {}. Falling back to default grid.", path, e);
            *grid = Grid::init(config.cols, config.rows);
            state.water_flow = false;
            state.gate_progress = 0;
        }
    }
}

fn try_load_level(
    path: &PathBuf,
    grid: &mut Grid,
    inlet_mode: &mut InletMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read_to_string(path)?;
    let level: LevelData = serde_json::from_str(&json)?;

    *grid = Grid::blank(level.width, level.height);
    for placement in &level.cells {
        if placement.x < grid.width && placement.y < grid.height {
            grid.set_cell(placement.x, placement.y, placement.cell.clone());
        }
    }
    *inlet_mode = level.default_inlet_mode;
    Ok(())
}

pub fn setup_level(
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
    mut inlet_mode: ResMut<InletMode>,
    config: Res<GridConfig>,
    current_level: Res<CurrentLevel>,
) {
    load_level(&current_level.path, &mut grid, &mut state, &mut inlet_mode, &config);
}

pub struct LevelsPlugin {
    pub level_path: String,
}

impl Plugin for LevelsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CurrentLevel {
            path: PathBuf::from(&self.level_path),
        })
        .add_systems(Startup, setup_level.after(crate::grid::setup));
    }
}
