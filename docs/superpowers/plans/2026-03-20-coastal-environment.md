# Coastal Environment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the game from a plain grid into a warm Mediterranean coastal town with Rock/Sand terrain cell types, a JSON-driven level loader, and a sky-blue background.

**Architecture:** Add `Cell::Rock` and `Cell::Sand` to the simulation enum, build a `src/levels.rs` module that deserialises JSON level files into the `Grid`, and extend `render.rs` with two new materials and a `ClearColor` sky. The Reset button reloads the active level file instead of calling `Grid::init`.

**Tech Stack:** Rust, Bevy 0.15, serde_json (already in `Cargo.toml`)

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/simulation.rs` | Modify | Add `Cell::Rock`, `Cell::Sand`, `Grid::blank`; update all match arms |
| `src/levels.rs` | Create | `LevelData`, `CurrentLevel`, `LevelsPlugin`, `load_level` |
| `src/grid.rs` | Modify | Add `Serialize, Deserialize` to `InletMode`; make `setup` pub; fix `KeyR` handler |
| `src/main.rs` | Modify | Register `LevelsPlugin` |
| `src/render.rs` | Modify | Add Rock/Sand to `MaterialPalette`; add `ClearColor`; update match arms |
| `src/ui.rs` | Modify | Update `handle_reset` to reload level |
| `levels/coastal-bowl.json` | Create | Coastal bowl level layout |
| `levels/harbour-inlet.json` | Create | Minimal valid stub |

---

## Task 1: Add `Grid::blank` constructor

**Files:**
- Modify: `src/simulation.rs:34-65`

`Grid::blank` returns an all-`Air` grid with no hardcoded walls or reservoir. This is the foundation the level loader builds on.

- [ ] **Step 1: Write a failing test**

Add to the `#[cfg(test)]` module at the bottom of `src/simulation.rs`:

```rust
#[test]
fn test_grid_blank_is_all_air() {
    let grid = Grid::blank(5, 4);
    assert_eq!(grid.width, 5);
    assert_eq!(grid.height, 4);
    assert!(grid.cells.iter().all(|c| matches!(c, Cell::Air)));
}
```

- [ ] **Step 2: Run test — expect FAIL (no `blank` method yet)**

```bash
cargo test test_grid_blank_is_all_air
```

Expected: `error[E0599]: no method named 'blank' found`

- [ ] **Step 3: Add `Grid::blank` to `src/simulation.rs`**

Insert after the closing `}` of `Grid::init` (line 65), before `set_cell`:

```rust
/// Create a new grid with all cells set to `Air` and no hardcoded geometry.
/// Use this as the base when loading a level from a JSON file.
pub fn blank(width: usize, height: usize) -> Grid {
    Grid {
        width,
        height,
        cells: vec![Cell::Air; width * height],
    }
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test test_grid_blank_is_all_air
```

- [ ] **Step 5: Commit**

```bash
git add src/simulation.rs
git commit -m "feat: add Grid::blank constructor"
```

---

## Task 2: Add `Cell::Rock` and `Cell::Sand` variants

**Files:**
- Modify: `src/simulation.rs`

Add the two new variants to the `Cell` enum and update every exhaustive `match` arm in the file. The compiler will tell you every place you've missed — treat compiler errors as your checklist.

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` module in `src/simulation.rs`:

```rust
#[test]
fn test_rock_blocks_water_flow() {
    // A 1x3 grid: Water at top, Rock in middle, Air at bottom.
    // After one step, no water should reach the bottom cell.
    let mut grid = Grid::blank(1, 3);
    grid.set_cell(0, 0, Cell::Water(MAX_WATER_KG));
    grid.set_cell(0, 1, Cell::Rock);
    // y=2 stays Air
    let result = step_simulation(&grid);
    assert!(
        matches!(result[2], Cell::Air),
        "Rock should block water flow: got {:?}", result[2]
    );
}

#[test]
fn test_sand_allows_water_flow() {
    // A 1x3 grid: Water at top, Sand in middle, Air at bottom.
    // After one step, water should be able to reach the Sand cell.
    let mut grid = Grid::blank(1, 3);
    grid.set_cell(0, 0, Cell::Water(MAX_WATER_KG));
    grid.set_cell(0, 1, Cell::Sand);
    // y=2 stays Air
    let result = step_simulation(&grid);
    assert!(
        matches!(result[1], Cell::Water(_) | Cell::Sand),
        "Sand should not block water flow"
    );
}
```

- [ ] **Step 2: Run tests — expect FAIL (variants don't exist yet)**

```bash
cargo test test_rock_blocks_water_flow test_sand_allows_water_flow
```

- [ ] **Step 3: Add `Rock` and `Sand` to the `Cell` enum**

In `src/simulation.rs`, add after `Building` (line 24):

```rust
/// Natural terrain — impassable cliff or rock face. Identical to Wall in simulation.
Rock,
/// Natural terrain — passable beach or sea floor. Identical to Air in simulation.
Sand,
```

- [ ] **Step 4: Fix all compiler errors — update every match arm**

Run `cargo build` and fix each error in turn. The required changes are:

**`water_fill` (line 78):** Add to the `None` arm:
```rust
Cell::Air | Cell::Object(_) | Cell::Wall | Cell::Drain | Cell::Building { .. }
    | Cell::Rock | Cell::Sand => None,
```

**`flow_capacity` (line 86):** Rock → `None` (blocks flow, same as Wall). Sand → `Some(0.0)` (passable, same as Air):
```rust
Cell::Air | Cell::Sand => Some(0.0),
Cell::Drain => Some(0.0),
Cell::Object(_) | Cell::Wall | Cell::Spring | Cell::Building { .. } | Cell::Rock => None,
```

**`step_simulation` restore block (lines 155-158):** Rock and Sand must survive the simulation step unchanged (like Spring/Drain):
```rust
Cell::Spring => new_cells[i] = Cell::Spring,
Cell::Drain => new_cells[i] = Cell::Drain,
Cell::Rock => new_cells[i] = Cell::Rock,
Cell::Sand => new_cells[i] = Cell::Sand,
_ => {}
```

**`build_depth_pressure` (line 213):** Rock clears pressure (same as Wall). Sand clears pressure (same as Air/Drain):
```rust
Cell::Wall | Cell::Rock => {
    depth[y * width + x] = 0.0;
    water_below.clear();
}
Cell::Air | Cell::Drain | Cell::Sand => {
    water_below.clear();
    depth[y * width + x] = 0.0;
}
```

**`build_flow_distance` (line 278):** Rock blocks BFS (same as Wall):
```rust
Cell::Wall | Cell::Building { .. } | Cell::Rock => {} // block flow path
```

**`is_blocked` closure in `step_objects` (line 494):** Rock is an obstacle objects cannot enter:
```rust
matches!(new_cells[idx], Cell::Wall | Cell::Spring | Cell::Drain | Cell::Building { .. } | Cell::Rock)
    || (matches!(new_cells[idx], Cell::Object(_)) && !moved_srcs.contains(&idx))
```

**`flow_water` match in `src/grid.rs` (line 416):** This function has an exhaustive match over every cell in the inlet row. Rock and Sand must survive unchanged. Add these arms alongside the existing `Cell::Wall` arm:
```rust
Cell::Rock => Cell::Rock,
Cell::Sand => Cell::Sand,
```

**`step_buildings` debris guard in `src/simulation.rs` (line 586):** Debris must not overwrite Rock cells. Add `Cell::Rock` to the existing guard:
```rust
Cell::Wall | Cell::Spring | Cell::Drain | Cell::Building { .. } | Cell::Rock => continue,
```

- [ ] **Step 5: Run tests — expect PASS**

```bash
cargo test test_rock_blocks_water_flow test_sand_allows_water_flow
```

- [ ] **Step 6: Run all tests to check nothing is broken**

```bash
cargo test
```

- [ ] **Step 7: Commit**

```bash
git add src/simulation.rs
git commit -m "feat: add Cell::Rock and Cell::Sand terrain types"
```

---

## Task 3: Add `Serialize/Deserialize` to `InletMode`

**Files:**
- Modify: `src/grid.rs:71`

The level loader will deserialise `InletMode` from JSON. It currently doesn't derive those traits.

- [ ] **Step 1: Add derives to `InletMode`**

In `src/grid.rs`, change line 71:

```rust
// Before:
#[derive(Resource, PartialEq, Clone, Default)]
pub enum InletMode {

// After:
#[derive(Resource, PartialEq, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum InletMode {
```

- [ ] **Step 2: Confirm it compiles**

```bash
cargo build
```

- [ ] **Step 3: Commit**

```bash
git add src/grid.rs
git commit -m "feat: derive Serialize/Deserialize for InletMode"
```

---

## Task 4: Create `src/levels.rs`

**Files:**
- Create: `src/levels.rs`
- Modify: `src/main.rs`

This module loads a JSON level file into the `Grid` resource at startup.

- [ ] **Step 1: Create `src/levels.rs`**

```rust
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

fn setup_level(
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
    mut inlet_mode: ResMut<InletMode>,
    config: Res<GridConfig>,
    current_level: Res<CurrentLevel>,
) {
    load_level(&current_level.path, &mut grid, &mut state, &mut inlet_mode, &config);
}

pub struct LevelsPlugin;

impl Plugin for LevelsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CurrentLevel {
            path: PathBuf::from("levels/coastal-bowl.json"),
        })
        .add_systems(Startup, setup_level.after(crate::grid::setup));
    }
}
```

- [ ] **Step 2: Register `LevelsPlugin` in `src/main.rs`**

Add `mod levels;` to the module list and `.add_plugins(LevelsPlugin)` to the app:

```rust
// Add after `mod undo;`:
mod levels;

// Add after `.add_plugins(GridPlugin)`:
.add_plugins(crate::levels::LevelsPlugin)
```

- [ ] **Step 3: Make `setup` public in `src/grid.rs`**

The `LevelsPlugin` needs to order its system after `crate::grid::setup`, which requires the function to be accessible from outside the module. In `src/grid.rs` line 143, change:

```rust
// Before:
fn setup(mut commands: Commands, config: Res<GridConfig>) {

// After:
pub fn setup(mut commands: Commands, config: Res<GridConfig>) {
```

- [ ] **Step 4: Confirm it compiles**

```bash
cargo build
```

- [ ] **Step 5: Commit**

```bash
git add src/levels.rs src/main.rs src/grid.rs
git commit -m "feat: add LevelsPlugin with JSON level loading"
```

---

## Task 5: Create level JSON files

**Files:**
- Create: `levels/coastal-bowl.json`
- Create: `levels/harbour-inlet.json`

The coastal bowl is 60 columns × 33 rows. Grid coordinates: x=0 is left, y=0 is top (inlet row).

- [ ] **Step 1: Create `levels/harbour-inlet.json`** (stub — no cells)

```json
{
  "name": "Harbour Inlet",
  "width": 60,
  "height": 33,
  "default_inlet_mode": "Sine",
  "cells": []
}
```

- [ ] **Step 2: Create `levels/coastal-bowl.json`**

The layout (all coordinates are `{x, y}` where y=0 is the top/inlet row):

```json
{
  "name": "Coastal Bowl",
  "width": 60,
  "height": 33,
  "default_inlet_mode": "Sine",
  "cells": [
    // --- Rock rim: top row (y=0) with 4-cell inlet gap at centre (x=28..31) ---
    // Left of gap: x=0..27
    // Right of gap: x=32..59
    // Left column: x=0, y=0..32
    // Right column: x=59, y=0..32
    // Bottom row: x=1..58, y=32 (floor)

    // --- Sand beach: bottom 3 rows (y=30,31,32) interior ---
    // Tapering: y=32 full width, y=31 x=1..55, y=30 x=1..45

    // --- Ocean inlet: bottom-left corner 6×6 (x=1..6, y=27..32) pre-filled Water ---

    // --- Rock outcrops: 3 small clusters in the interior ---
  ]
}
```

The JSON file is generated programmatically. Write a helper script to produce it, OR write it by hand using the layout above. The simplest approach: write it directly. Here is the full cell list:

```json
{
  "name": "Coastal Bowl",
  "width": 60,
  "height": 33,
  "default_inlet_mode": "Sine",
  "cells": [
    {"x":0,"y":0,"cell":"Rock"},{"x":1,"y":0,"cell":"Rock"},{"x":2,"y":0,"cell":"Rock"},{"x":3,"y":0,"cell":"Rock"},{"x":4,"y":0,"cell":"Rock"},{"x":5,"y":0,"cell":"Rock"},{"x":6,"y":0,"cell":"Rock"},{"x":7,"y":0,"cell":"Rock"},{"x":8,"y":0,"cell":"Rock"},{"x":9,"y":0,"cell":"Rock"},{"x":10,"y":0,"cell":"Rock"},{"x":11,"y":0,"cell":"Rock"},{"x":12,"y":0,"cell":"Rock"},{"x":13,"y":0,"cell":"Rock"},{"x":14,"y":0,"cell":"Rock"},{"x":15,"y":0,"cell":"Rock"},{"x":16,"y":0,"cell":"Rock"},{"x":17,"y":0,"cell":"Rock"},{"x":18,"y":0,"cell":"Rock"},{"x":19,"y":0,"cell":"Rock"},{"x":20,"y":0,"cell":"Rock"},{"x":21,"y":0,"cell":"Rock"},{"x":22,"y":0,"cell":"Rock"},{"x":23,"y":0,"cell":"Rock"},{"x":24,"y":0,"cell":"Rock"},{"x":25,"y":0,"cell":"Rock"},{"x":26,"y":0,"cell":"Rock"},{"x":27,"y":0,"cell":"Rock"},
    {"x":32,"y":0,"cell":"Rock"},{"x":33,"y":0,"cell":"Rock"},{"x":34,"y":0,"cell":"Rock"},{"x":35,"y":0,"cell":"Rock"},{"x":36,"y":0,"cell":"Rock"},{"x":37,"y":0,"cell":"Rock"},{"x":38,"y":0,"cell":"Rock"},{"x":39,"y":0,"cell":"Rock"},{"x":40,"y":0,"cell":"Rock"},{"x":41,"y":0,"cell":"Rock"},{"x":42,"y":0,"cell":"Rock"},{"x":43,"y":0,"cell":"Rock"},{"x":44,"y":0,"cell":"Rock"},{"x":45,"y":0,"cell":"Rock"},{"x":46,"y":0,"cell":"Rock"},{"x":47,"y":0,"cell":"Rock"},{"x":48,"y":0,"cell":"Rock"},{"x":49,"y":0,"cell":"Rock"},{"x":50,"y":0,"cell":"Rock"},{"x":51,"y":0,"cell":"Rock"},{"x":52,"y":0,"cell":"Rock"},{"x":53,"y":0,"cell":"Rock"},{"x":54,"y":0,"cell":"Rock"},{"x":55,"y":0,"cell":"Rock"},{"x":56,"y":0,"cell":"Rock"},{"x":57,"y":0,"cell":"Rock"},{"x":58,"y":0,"cell":"Rock"},{"x":59,"y":0,"cell":"Rock"},
    {"x":0,"y":1,"cell":"Rock"},{"x":59,"y":1,"cell":"Rock"},
    {"x":0,"y":2,"cell":"Rock"},{"x":59,"y":2,"cell":"Rock"},
    {"x":0,"y":3,"cell":"Rock"},{"x":59,"y":3,"cell":"Rock"},
    {"x":0,"y":4,"cell":"Rock"},{"x":59,"y":4,"cell":"Rock"},
    {"x":0,"y":5,"cell":"Rock"},{"x":59,"y":5,"cell":"Rock"},
    {"x":0,"y":6,"cell":"Rock"},{"x":59,"y":6,"cell":"Rock"},
    {"x":0,"y":7,"cell":"Rock"},{"x":59,"y":7,"cell":"Rock"},
    {"x":0,"y":8,"cell":"Rock"},{"x":59,"y":8,"cell":"Rock"},
    {"x":0,"y":9,"cell":"Rock"},{"x":59,"y":9,"cell":"Rock"},
    {"x":0,"y":10,"cell":"Rock"},{"x":59,"y":10,"cell":"Rock"},
    {"x":0,"y":11,"cell":"Rock"},{"x":59,"y":11,"cell":"Rock"},
    {"x":0,"y":12,"cell":"Rock"},{"x":59,"y":12,"cell":"Rock"},
    {"x":0,"y":13,"cell":"Rock"},{"x":59,"y":13,"cell":"Rock"},
    {"x":0,"y":14,"cell":"Rock"},{"x":59,"y":14,"cell":"Rock"},
    {"x":0,"y":15,"cell":"Rock"},{"x":59,"y":15,"cell":"Rock"},
    {"x":0,"y":16,"cell":"Rock"},{"x":59,"y":16,"cell":"Rock"},
    {"x":0,"y":17,"cell":"Rock"},{"x":59,"y":17,"cell":"Rock"},
    {"x":0,"y":18,"cell":"Rock"},{"x":59,"y":18,"cell":"Rock"},
    {"x":0,"y":19,"cell":"Rock"},{"x":59,"y":19,"cell":"Rock"},
    {"x":0,"y":20,"cell":"Rock"},{"x":59,"y":20,"cell":"Rock"},
    {"x":0,"y":21,"cell":"Rock"},{"x":59,"y":21,"cell":"Rock"},
    {"x":0,"y":22,"cell":"Rock"},{"x":59,"y":22,"cell":"Rock"},
    {"x":0,"y":23,"cell":"Rock"},{"x":59,"y":23,"cell":"Rock"},
    {"x":0,"y":24,"cell":"Rock"},{"x":59,"y":24,"cell":"Rock"},
    {"x":0,"y":25,"cell":"Rock"},{"x":59,"y":25,"cell":"Rock"},
    {"x":0,"y":26,"cell":"Rock"},{"x":59,"y":26,"cell":"Rock"},
    {"x":0,"y":27,"cell":"Rock"},{"x":59,"y":27,"cell":"Rock"},
    {"x":0,"y":28,"cell":"Rock"},{"x":59,"y":28,"cell":"Rock"},
    {"x":0,"y":29,"cell":"Rock"},{"x":59,"y":29,"cell":"Rock"},
    {"x":0,"y":30,"cell":"Rock"},{"x":59,"y":30,"cell":"Rock"},
    {"x":0,"y":31,"cell":"Rock"},{"x":59,"y":31,"cell":"Rock"},
    {"x":0,"y":32,"cell":"Rock"},{"x":59,"y":32,"cell":"Rock"},
    {"x":1,"y":32,"cell":"Rock"},{"x":2,"y":32,"cell":"Rock"},{"x":3,"y":32,"cell":"Rock"},{"x":4,"y":32,"cell":"Rock"},{"x":5,"y":32,"cell":"Rock"},{"x":6,"y":32,"cell":"Rock"},{"x":7,"y":32,"cell":"Rock"},{"x":8,"y":32,"cell":"Rock"},{"x":9,"y":32,"cell":"Rock"},{"x":10,"y":32,"cell":"Rock"},{"x":11,"y":32,"cell":"Rock"},{"x":12,"y":32,"cell":"Rock"},{"x":13,"y":32,"cell":"Rock"},{"x":14,"y":32,"cell":"Rock"},{"x":15,"y":32,"cell":"Rock"},{"x":16,"y":32,"cell":"Rock"},{"x":17,"y":32,"cell":"Rock"},{"x":18,"y":32,"cell":"Rock"},{"x":19,"y":32,"cell":"Rock"},{"x":20,"y":32,"cell":"Rock"},{"x":21,"y":32,"cell":"Rock"},{"x":22,"y":32,"cell":"Rock"},{"x":23,"y":32,"cell":"Rock"},{"x":24,"y":32,"cell":"Rock"},{"x":25,"y":32,"cell":"Rock"},{"x":26,"y":32,"cell":"Rock"},{"x":27,"y":32,"cell":"Rock"},{"x":28,"y":32,"cell":"Rock"},{"x":29,"y":32,"cell":"Rock"},{"x":30,"y":32,"cell":"Rock"},{"x":31,"y":32,"cell":"Rock"},{"x":32,"y":32,"cell":"Rock"},{"x":33,"y":32,"cell":"Rock"},{"x":34,"y":32,"cell":"Rock"},{"x":35,"y":32,"cell":"Rock"},{"x":36,"y":32,"cell":"Rock"},{"x":37,"y":32,"cell":"Rock"},{"x":38,"y":32,"cell":"Rock"},{"x":39,"y":32,"cell":"Rock"},{"x":40,"y":32,"cell":"Rock"},{"x":41,"y":32,"cell":"Rock"},{"x":42,"y":32,"cell":"Rock"},{"x":43,"y":32,"cell":"Rock"},{"x":44,"y":32,"cell":"Rock"},{"x":45,"y":32,"cell":"Rock"},{"x":46,"y":32,"cell":"Rock"},{"x":47,"y":32,"cell":"Rock"},{"x":48,"y":32,"cell":"Rock"},{"x":49,"y":32,"cell":"Rock"},{"x":50,"y":32,"cell":"Rock"},{"x":51,"y":32,"cell":"Rock"},{"x":52,"y":32,"cell":"Rock"},{"x":53,"y":32,"cell":"Rock"},{"x":54,"y":32,"cell":"Rock"},{"x":55,"y":32,"cell":"Rock"},{"x":56,"y":32,"cell":"Rock"},{"x":57,"y":32,"cell":"Rock"},{"x":58,"y":32,"cell":"Rock"},
    {"x":1,"y":31,"cell":"Sand"},{"x":2,"y":31,"cell":"Sand"},{"x":3,"y":31,"cell":"Sand"},{"x":4,"y":31,"cell":"Sand"},{"x":5,"y":31,"cell":"Sand"},{"x":6,"y":31,"cell":"Sand"},{"x":7,"y":31,"cell":"Sand"},{"x":8,"y":31,"cell":"Sand"},{"x":9,"y":31,"cell":"Sand"},{"x":10,"y":31,"cell":"Sand"},{"x":11,"y":31,"cell":"Sand"},{"x":12,"y":31,"cell":"Sand"},{"x":13,"y":31,"cell":"Sand"},{"x":14,"y":31,"cell":"Sand"},{"x":15,"y":31,"cell":"Sand"},{"x":16,"y":31,"cell":"Sand"},{"x":17,"y":31,"cell":"Sand"},{"x":18,"y":31,"cell":"Sand"},{"x":19,"y":31,"cell":"Sand"},{"x":20,"y":31,"cell":"Sand"},{"x":21,"y":31,"cell":"Sand"},{"x":22,"y":31,"cell":"Sand"},{"x":23,"y":31,"cell":"Sand"},{"x":24,"y":31,"cell":"Sand"},{"x":25,"y":31,"cell":"Sand"},{"x":26,"y":31,"cell":"Sand"},{"x":27,"y":31,"cell":"Sand"},{"x":28,"y":31,"cell":"Sand"},{"x":29,"y":31,"cell":"Sand"},{"x":30,"y":31,"cell":"Sand"},{"x":31,"y":31,"cell":"Sand"},{"x":32,"y":31,"cell":"Sand"},{"x":33,"y":31,"cell":"Sand"},{"x":34,"y":31,"cell":"Sand"},{"x":35,"y":31,"cell":"Sand"},{"x":36,"y":31,"cell":"Sand"},{"x":37,"y":31,"cell":"Sand"},{"x":38,"y":31,"cell":"Sand"},{"x":39,"y":31,"cell":"Sand"},{"x":40,"y":31,"cell":"Sand"},{"x":41,"y":31,"cell":"Sand"},{"x":42,"y":31,"cell":"Sand"},{"x":43,"y":31,"cell":"Sand"},{"x":44,"y":31,"cell":"Sand"},{"x":45,"y":31,"cell":"Sand"},{"x":46,"y":31,"cell":"Sand"},{"x":47,"y":31,"cell":"Sand"},{"x":48,"y":31,"cell":"Sand"},{"x":49,"y":31,"cell":"Sand"},{"x":50,"y":31,"cell":"Sand"},{"x":51,"y":31,"cell":"Sand"},{"x":52,"y":31,"cell":"Sand"},{"x":53,"y":31,"cell":"Sand"},{"x":54,"y":31,"cell":"Sand"},{"x":55,"y":31,"cell":"Sand"},{"x":56,"y":31,"cell":"Sand"},{"x":57,"y":31,"cell":"Sand"},{"x":58,"y":31,"cell":"Sand"},
    {"x":1,"y":30,"cell":"Sand"},{"x":2,"y":30,"cell":"Sand"},{"x":3,"y":30,"cell":"Sand"},{"x":4,"y":30,"cell":"Sand"},{"x":5,"y":30,"cell":"Sand"},{"x":6,"y":30,"cell":"Sand"},{"x":7,"y":30,"cell":"Sand"},{"x":8,"y":30,"cell":"Sand"},{"x":9,"y":30,"cell":"Sand"},{"x":10,"y":30,"cell":"Sand"},{"x":11,"y":30,"cell":"Sand"},{"x":12,"y":30,"cell":"Sand"},{"x":13,"y":30,"cell":"Sand"},{"x":14,"y":30,"cell":"Sand"},{"x":15,"y":30,"cell":"Sand"},{"x":16,"y":30,"cell":"Sand"},{"x":17,"y":30,"cell":"Sand"},{"x":18,"y":30,"cell":"Sand"},{"x":19,"y":30,"cell":"Sand"},{"x":20,"y":30,"cell":"Sand"},{"x":21,"y":30,"cell":"Sand"},{"x":22,"y":30,"cell":"Sand"},{"x":23,"y":30,"cell":"Sand"},{"x":24,"y":30,"cell":"Sand"},{"x":25,"y":30,"cell":"Sand"},{"x":26,"y":30,"cell":"Sand"},{"x":27,"y":30,"cell":"Sand"},{"x":28,"y":30,"cell":"Sand"},{"x":29,"y":30,"cell":"Sand"},{"x":30,"y":30,"cell":"Sand"},{"x":31,"y":30,"cell":"Sand"},{"x":32,"y":30,"cell":"Sand"},{"x":33,"y":30,"cell":"Sand"},{"x":34,"y":30,"cell":"Sand"},{"x":35,"y":30,"cell":"Sand"},{"x":36,"y":30,"cell":"Sand"},{"x":37,"y":30,"cell":"Sand"},{"x":38,"y":30,"cell":"Sand"},{"x":39,"y":30,"cell":"Sand"},{"x":40,"y":30,"cell":"Sand"},{"x":41,"y":30,"cell":"Sand"},{"x":42,"y":30,"cell":"Sand"},{"x":43,"y":30,"cell":"Sand"},{"x":44,"y":30,"cell":"Sand"},{"x":45,"y":30,"cell":"Sand"},
    {"x":1,"y":29,"cell":"Sand"},{"x":2,"y":29,"cell":"Sand"},{"x":3,"y":29,"cell":"Sand"},{"x":4,"y":29,"cell":"Sand"},{"x":5,"y":29,"cell":"Sand"},
    {"x":1,"y":28,"cell":{"Water":1.0}},{"x":2,"y":28,"cell":{"Water":1.0}},{"x":3,"y":28,"cell":{"Water":1.0}},{"x":4,"y":28,"cell":{"Water":1.0}},{"x":5,"y":28,"cell":{"Water":1.0}},{"x":6,"y":28,"cell":{"Water":1.0}},
    {"x":1,"y":27,"cell":{"Water":1.0}},{"x":2,"y":27,"cell":{"Water":1.0}},{"x":3,"y":27,"cell":{"Water":1.0}},{"x":4,"y":27,"cell":{"Water":1.0}},{"x":5,"y":27,"cell":{"Water":1.0}},{"x":6,"y":27,"cell":{"Water":1.0}},
    {"x":1,"y":26,"cell":{"Water":1.0}},{"x":2,"y":26,"cell":{"Water":1.0}},{"x":3,"y":26,"cell":{"Water":1.0}},{"x":4,"y":26,"cell":{"Water":1.0}},{"x":5,"y":26,"cell":{"Water":1.0}},{"x":6,"y":26,"cell":{"Water":1.0}},
    {"x":1,"y":25,"cell":{"Water":1.0}},{"x":2,"y":25,"cell":{"Water":1.0}},{"x":3,"y":25,"cell":{"Water":1.0}},{"x":4,"y":25,"cell":{"Water":1.0}},{"x":5,"y":25,"cell":{"Water":1.0}},{"x":6,"y":25,"cell":{"Water":1.0}},
    {"x":1,"y":24,"cell":{"Water":1.0}},{"x":2,"y":24,"cell":{"Water":1.0}},{"x":3,"y":24,"cell":{"Water":1.0}},{"x":4,"y":24,"cell":{"Water":1.0}},{"x":5,"y":24,"cell":{"Water":1.0}},{"x":6,"y":24,"cell":{"Water":1.0}},
    {"x":1,"y":23,"cell":{"Water":1.0}},{"x":2,"y":23,"cell":{"Water":1.0}},{"x":3,"y":23,"cell":{"Water":1.0}},{"x":4,"y":23,"cell":{"Water":1.0}},{"x":5,"y":23,"cell":{"Water":1.0}},{"x":6,"y":23,"cell":{"Water":1.0}},
    {"x":20,"y":15,"cell":"Rock"},{"x":21,"y":15,"cell":"Rock"},{"x":20,"y":16,"cell":"Rock"},
    {"x":40,"y":10,"cell":"Rock"},{"x":40,"y":11,"cell":"Rock"},{"x":41,"y":11,"cell":"Rock"},
    {"x":35,"y":22,"cell":"Rock"},{"x":36,"y":22,"cell":"Rock"},{"x":35,"y":23,"cell":"Rock"},{"x":36,"y":23,"cell":"Rock"}
  ]
}
```

- [ ] **Step 3: Verify JSON is valid**

```bash
python3 -c "import json,sys; json.load(open('levels/coastal-bowl.json')); print('valid')"
python3 -c "import json,sys; json.load(open('levels/harbour-inlet.json')); print('valid')"
```

- [ ] **Step 4: Run the game and verify the level loads**

```bash
cargo run
```

Expected: game starts, grid shows the bowl shape (Rock rim, Sand beach, water in bottom-left corner).

- [ ] **Step 5: Commit**

```bash
git add levels/
git commit -m "feat: add coastal-bowl and harbour-inlet level files"
```

---

## Task 6: Update `handle_reset` to reload the level

**Files:**
- Modify: `src/ui.rs:936-948`

- [ ] **Step 1: Update `handle_reset` signature and body**

Find `fn handle_reset` in `src/ui.rs` and replace the function:

```rust
fn handle_reset(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<ResetButton>)>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
    mut inlet_mode: ResMut<InletMode>,
    mut undo_stack: ResMut<crate::undo::UndoStack>,
    config: Res<GridConfig>,
    current_level: Res<crate::levels::CurrentLevel>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            crate::levels::load_level(
                &current_level.path,
                &mut grid,
                &mut state,
                &mut inlet_mode,
                &config,
            );
            undo_stack.clear();
        }
    }
}
```

- [ ] **Step 2: Fix imports in `src/ui.rs`**

The current import at the top of `src/ui.rs` is:
```rust
use crate::grid::{GameState, InletMode, PANEL_WIDTH, SelectedTool, ViewMode};
```

`GridConfig` is not imported. Add it:
```rust
use crate::grid::{GameState, GridConfig, InletMode, PANEL_WIDTH, SelectedTool, ViewMode};
```

- [ ] **Step 3: Fix the `KeyR` keyboard shortcut in `src/grid.rs`**

The `handle_input` function in `src/grid.rs` at line 330 has a keyboard shortcut that resets the grid directly, bypassing the level loader. Update it to call `load_level` instead.

First add `CurrentLevel` to the `handle_input` system parameters (find the existing `fn handle_input(` signature and add):
```rust
current_level: Res<crate::levels::CurrentLevel>,
config: Res<GridConfig>,
mut inlet_mode: ResMut<InletMode>,
```

Then replace the `KeyR` block (lines 330-335):
```rust
// Before:
if keyboard.just_pressed(KeyCode::KeyR) {
    *grid = Grid::init(grid.width, grid.height);
    state.water_flow = false;
    state.gate_progress = 0;
    undo_stack.clear();
}

// After:
if keyboard.just_pressed(KeyCode::KeyR) {
    crate::levels::load_level(
        &current_level.path,
        &mut grid,
        &mut state,
        &mut inlet_mode,
        &config,
    );
    undo_stack.clear();
}
```

- [ ] **Step 4: Build and confirm no errors**

```bash
cargo build
```

- [ ] **Step 5: Run and test Reset**

```bash
cargo run
```

Place some objects, press the Reset button — the level should reload to the coastal bowl layout. Also press `R` on the keyboard — should produce the same result.

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs src/grid.rs
git commit -m "feat: reset (button + R key) reloads level file instead of blank grid"
```

---

## Task 7: Add Rock/Sand materials and sky background to `render.rs`

**Files:**
- Modify: `src/render.rs`

This task adds the visual theming: two new materials in `MaterialPalette`, new match arms in every cell-rendering function, a placement guard for Rock/Sand, and the `ClearColor` sky.

- [ ] **Step 1: Add `rock` and `sand` fields to `MaterialPalette`**

In `src/render.rs`, add to the `MaterialPalette` struct (after line 53):

```rust
pub rock: Handle<StandardMaterial>,
pub sand: Handle<StandardMaterial>,
```

- [ ] **Step 2: Create the materials in `build_material_palette`**

Find the `build_material_palette` function. Before `MaterialPalette {`, add:

```rust
let rock = materials.add(StandardMaterial {
    base_color: Color::srgb(0.478, 0.416, 0.353), // #7a6a5a
    ..default()
});
let sand = materials.add(StandardMaterial {
    base_color: Color::srgb(0.831, 0.667, 0.416), // #d4aa6a
    ..default()
});
```

And add both to the `MaterialPalette { ... }` struct literal:

```rust
rock,
sand,
```

- [ ] **Step 3: Add match arms in `render_grid`**

In `render_grid`, inside the `match cell { ... }` block (after `Cell::Building`), add:

```rust
Cell::Rock => (1.0, &palette.rock),
Cell::Sand => (0.2, &palette.sand),
```

- [ ] **Step 4: Add match arms in `render_heat_grid_3d`**

In `render_heat_grid_3d`, inside the `let h = match &grid.cells[idx] { ... }` block, add:

```rust
Cell::Rock => 1.0,
Cell::Sand => 0.2,
```

- [ ] **Step 5: Add match arms in `cell_surface_y`**

In `cell_surface_y`, inside the `let h = match cell { ... }` block, add:

```rust
Cell::Rock => 1.0,
Cell::Sand => 0.2,
```

- [ ] **Step 6: Guard Rock/Sand from player placement**

In `src/grid.rs`, the placement guard at line 219 currently only blocks `Cell::Wall`. Update it to also block `Cell::Rock` and `Cell::Sand`:

```rust
// Before:
&& !matches!(grid.get_cell(bx, by), Cell::Wall)

// After:
&& !matches!(grid.get_cell(bx, by), Cell::Wall | Cell::Rock | Cell::Sand)
```

- [ ] **Step 7: Set `ClearColor` sky in `setup_render`**

In `src/render.rs`, in `setup_render`, add at the very start of the function (before anything else):

```rust
commands.insert_resource(ClearColor(Color::srgb(0.357, 0.639, 0.851))); // #5ba3d9 sky blue
```

- [ ] **Step 8: Build**

```bash
cargo build
```

- [ ] **Step 9: Run and verify visuals**

```bash
cargo run
```

Expected:
- Background is sky blue
- Rock rim cells are stone grey-brown
- Sand beach cells are warm tan
- Bowl shape is visible
- Placing/erasing does not affect Rock or Sand cells

- [ ] **Step 10: Commit**

```bash
git add src/render.rs src/grid.rs
git commit -m "feat: add Rock/Sand materials and sky blue ClearColor"
```

---

## Task 8: Final build, test, and PR

- [ ] **Step 1: Run full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 2: Run the game — full manual check**

```bash
cargo run
```

Checklist:
- [ ] Sky is blue
- [ ] Bowl shape visible on startup (Rock rim, Sand beach, water in bottom-left)
- [ ] Water flows in from the inlet gap at the top centre
- [ ] Water spreads and fills the bowl
- [ ] Objects can be placed in the open interior cells
- [ ] Objects cannot be placed on Rock or Sand cells
- [ ] Reset reloads the coastal bowl layout
- [ ] Undo stack cleared after Reset
- [ ] Save/load still works (Cmd+S / Cmd+O)

- [ ] **Step 3: Update `PROGRESS.md`** with what was learned and built

- [ ] **Step 4: Create PR**

```bash
gh pr create --title "feat: coastal environment (Rock/Sand cells, level loader, sky)" \
  --body "Adds Rock and Sand cell types, a JSON level loader, and the Coastal Bowl level. Sky background set to Mediterranean blue."
```
