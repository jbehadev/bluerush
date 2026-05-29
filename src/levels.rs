use bevy::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;

use crate::grid::{GameState, GridConfig, InletMode};
use crate::simulation::Cell;
use crate::simulation3d::Grid3D;

/// Deserialised representation of a `levels/*.json` file.
/// The `depth` and `z` fields are optional for backwards-compatibility
/// with existing 2D level files (depth defaults to 1, z defaults to 0).
#[derive(Deserialize)]
pub struct LevelData {
    pub name: String,
    pub width: usize,
    pub height: usize,
    #[serde(default)]
    pub depth: Option<usize>,
    pub default_inlet_mode: InletMode,
    pub cells: Vec<CellPlacement>,
}

#[derive(Deserialize)]
pub struct CellPlacement {
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub z: Option<usize>,
    pub cell: Cell,
}

#[derive(Resource)]
pub struct CurrentLevel {
    pub path: PathBuf,
}

pub fn load_level(
    path: &PathBuf,
    grid: &mut Grid3D,
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
            warn!("Failed to load level {:?}: {}. Falling back to blank grid.", path, e);
            *grid = Grid3D::blank(config.cols, config.rows, config.depth);
            state.water_flow = false;
            state.gate_progress = 0;
        }
    }
}

fn try_load_level(
    path: &PathBuf,
    grid: &mut Grid3D,
    inlet_mode: &mut InletMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read_to_string(path)?;
    let level: LevelData = serde_json::from_str(&json)?;

    let depth = level.depth.unwrap_or(1);
    *grid = Grid3D::blank(level.width, level.height, depth);
    for p in &level.cells {
        let z = p.z.unwrap_or(0);
        if p.x < grid.width && p.y < grid.height && z < grid.depth {
            grid.set_cell(p.x, p.y, z, p.cell.clone());
        }
    }
    *inlet_mode = level.default_inlet_mode;
    Ok(())
}

pub fn setup_level(
    mut grid: ResMut<Grid3D>,
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

// ---------------------------------------------------------------------------
// Level generation helper (run once to produce levels/valley-flood.json)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod level_gen {
    #[test]
    fn generate_valley_flood() {
        let (width, height, depth) = (40usize, 20usize, 40usize);
        let mut entries: Vec<String> = Vec::new();

        let rock = |x: usize, y: usize, z: usize| -> String {
            format!(r#"{{"x":{x},"y":{y},"z":{z},"cell":"Rock"}}"#)
        };

        // Rock floor at Y=0
        for z in 0..depth { for x in 0..width { entries.push(rock(x, 0, z)); } }

        // Rock walls Y=1..height-1 on all four XZ edges
        for y in 1..height {
            for z in 0..depth {
                entries.push(rock(0, y, z));
                entries.push(rock(width - 1, y, z));
            }
            for x in 1..width - 1 {
                entries.push(rock(x, y, 0));
                entries.push(rock(x, y, depth - 1));
            }
        }

        // Spring at (1, 2, 1) — water source in one corner, above the floor
        entries.push(r#"{"x":1,"y":2,"z":1,"cell":"Spring"}"#.to_string());

        let json = format!(
            concat!(
                r#"{{"name":"Valley Flood","width":{w},"height":{h},"depth":{d},"#,
                r#""default_inlet_mode":"Flood","cells":[{cells}]}}"#
            ),
            w = width, h = height, d = depth,
            cells = entries.join(",")
        );

        std::fs::write("levels/valley-flood.json", &json)
            .expect("Failed to write levels/valley-flood.json");
        println!("Written {} bytes to levels/valley-flood.json", json.len());
    }
}
