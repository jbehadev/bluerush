# Coastal Environment Design

**Date:** 2026-03-20
**Status:** Approved

## Overview

Transform the BlueRush game environment from a plain rectangular grid into a coastal town setting. The current flat grey box becomes a warm Mediterranean bowl — a low-lying town caught between rock cliffs and the sea, threatened by rising water.

This covers three axes simultaneously:
1. **Terrain shape** — Rock and Sand cell types form a physical bowl
2. **Visual theming** — Warm Mediterranean colour palette with a sky background colour
3. **Environmental storytelling** — The layout reads as a real place before a drop of water flows

## New Cell Types

Two new variants added to the `Cell` enum in `simulation.rs`:

### `Cell::Rock`
- **Purpose:** Permanent level-defined terrain (cliffs, rim walls, outcrops). Distinct from `Cell::Wall`, which represents man-made border walls and gate rows placed by `Grid::init`. Rock is natural terrain defined in level JSON files only.
- **Simulation behaviour:** Impassable — identical to `Cell::Wall` in all simulation functions:
  - `water_fill` → `None`
  - `flow_capacity` → `None` (blocks flow)
  - `build_depth_pressure` — clears depth accumulation (same path as `Cell::Wall`)
  - `step_objects` — treated as an obstacle; objects cannot move into it
- **Visual:** Full-height mesh (`transform.scale.y = 1.0`), stone grey-brown material (`#7a6a5a`)
- **Player interaction:** Cannot be placed or erased — level geometry only. Not available in the toolbar.

### `Cell::Sand`
- **Purpose:** Permanent, passable terrain (beach, sea floor, ground texture)
- **Simulation behaviour:** Passable — identical to `Cell::Air` in all simulation functions:
  - `water_fill` → `None` (no water stored)
  - `flow_capacity` → `Some(0.0)` (water flows through freely, same as Air)
  - `build_depth_pressure` — contributes 0.0 depth (same as Air)
  - `step_objects` — objects can move into it (same as Air)
- **Visual:** Flat low-profile mesh (`transform.scale.y = 0.2`), sandy tan material (`#d4aa6a`)
- **Player interaction:** Cannot be placed or erased — level geometry only. Not available in the toolbar.

**Serialisation:** `Cell` already derives `Serialize, Deserialize`. Rock and Sand are added as unit variants; they round-trip correctly through player save/load. A saved game that contains Rock/Sand cells will restore that terrain on load — this is intentional and correct.

## Level File Format

Level layouts are defined in JSON files under `levels/` in the project root:

```
levels/
  coastal-bowl.json
  harbour-inlet.json    ← minimal valid stub, to be filled in a future session
```

### Schema

```json
{
  "name": "Coastal Bowl",
  "width": 60,
  "height": 33,
  "default_inlet_mode": "Sine",
  "cells": [
    { "x": 0, "y": 0, "cell": "Rock" },
    { "x": 1, "y": 0, "cell": "Sand" },
    { "x": 5, "y": 3, "cell": { "Water": 1.0 } }
  ]
}
```

- `cells` is a sparse list — only non-Air cells need to be listed
- Cell values match the `Cell` enum variants: `"Rock"`, `"Sand"`, `"Air"`, `{ "Water": f32 }`, `{ "Object": f32 }`, `"Wall"`
- `default_inlet_mode` matches the `InletMode` enum variants: `"Flood"`, `"Sine"`, `"Random"`
- `width` and `height` in the level file override `config.yaml` (`grid_cols`, `grid_rows`) — the level file is the source of truth for grid dimensions when a level is loaded

### Grid Dimensions

When a level loads, grid dimensions come from the level JSON, not `config.yaml`. A new `Grid::blank(width, height)` constructor is added that returns an all-`Cell::Air` grid with no hardcoded walls or reservoir. The level loader calls `Grid::blank()` and then applies the sparse cell list from the JSON on top. Tile entities in `render.rs` are spawned based on whichever dimensions are active at startup.

`Grid::init` (which adds hardcoded border walls and reservoir) is preserved as the fallback used when no level file is present or loading fails.

### Coastal Bowl Layout (`coastal-bowl.json`)

- **Rock rim** — Rock cells along the entire left column, right column, and top row, with a 4-cell gap at the top-centre for the water inlet
- **Sand beach** — A 3-row band of Sand cells along the bottom, tapering up ~5 columns on the left side to suggest a shoreline
- **Ocean inlet** — Bottom-left corner (~6×6 cells) pre-filled with `Water(1.0)` representing the open sea
- **Rock outcrops** — 3–4 small clusters of Rock cells (2–4 cells each) scattered in the bowl interior
- **Open centre** — Remaining interior cells are `Air` — the town area where the player places buildings

### Harbour Inlet Stub (`harbour-inlet.json`)

A minimal valid file that loads without error. Defines the same dimensions as the bowl with all cells as Air and `"name": "Harbour Inlet"`. Layout to be authored in a future session.

## Level Loading (`src/levels.rs`)

A new `LevelsPlugin` is responsible for:

- **`LevelData` struct** — deserialised representation of a level JSON file (`name`, `width`, `height`, `default_inlet_mode`, `cells`)
- **`CurrentLevel` resource** — holds the path of the currently loaded level file
- **`load_level(path, grid, state, inlet_mode)` function** — reads JSON, calls `Grid::blank()`, applies cells, sets `InletMode` from `default_inlet_mode`. On error (missing file, malformed JSON): logs the error and falls back to `Grid::init(width, height)` using config dimensions.
- **`setup_level` system (Startup)** — loads `levels/coastal-bowl.json`
- **`handle_reset` modification (`ui.rs`)** — replaces `*grid = Grid::init(...)` with a call to `load_level` using the path stored in `CurrentLevel`. Also clears `UndoStack` and resets `state.water_flow = false` and `state.gate_progress = 0` (preserving existing reset behaviour).

This module is separate from `persistence.rs`, which handles player save/load and is unchanged.

## Visual Theming

### Colour Palette (Warm Mediterranean)

| Element | Colour | Hex |
|---------|--------|-----|
| Sky background | Bright blue | `#5ba3d9` |
| Rock | Stone grey-brown | `#7a6a5a` |
| Sand | Warm tan | `#d4aa6a` |
| Water (full) | Mediterranean blue | `#3a7fc1` |
| Water (froth) | Existing froth texture | — |

### Sky Background

Set Bevy's `ClearColor` resource to `#5ba3d9` in `render.rs` setup. This fills the camera background with the sky colour at zero implementation cost and works correctly with the existing 3D orthographic isometric camera regardless of pan or zoom. A gradient sky is out of scope.

### Material Palette

`Rock` and `Sand` `StandardMaterial` handles are added to the existing `MaterialPalette` resource in `render.rs`. The `render_grid` system already switches materials per cell type — it needs two new match arms for `Cell::Rock` and `Cell::Sand`.

## Out of Scope

- Player-placeable Sand or Rock tools
- Erosion mechanics (Sand eroding under water pressure)
- Animated sky, weather effects, or sky gradient
- Level select UI (future work — the JSON infrastructure enables it)
- Harbour Inlet level content (file is a minimal valid stub only)
