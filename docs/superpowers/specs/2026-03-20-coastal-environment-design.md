# Coastal Environment Design

**Date:** 2026-03-20
**Status:** Approved

## Overview

Transform the BlueRush game environment from a plain rectangular grid into a coastal town setting. The current flat grey box becomes a warm Mediterranean bowl — a low-lying town caught between rock cliffs and the sea, threatened by rising water.

This covers three axes simultaneously:
1. **Terrain shape** — Rock and Sand cell types form a physical bowl
2. **Visual theming** — Warm Mediterranean colour palette with a sky background
3. **Environmental storytelling** — The layout reads as a real place before a drop of water flows

## New Cell Types

Two new variants added to the `Cell` enum in `simulation.rs`:

### `Cell::Rock`
- **Purpose:** Permanent, impassable terrain (cliffs, rim walls, outcrops)
- **Simulation behaviour:** Identical to current `Object(9999)` border walls — water cannot enter, pressure physics ignore it, cannot be moved
- **Visual:** Full-height mesh, stone grey-brown (`#7a6a5a`)
- **Player interaction:** Cannot be placed or erased — level geometry only

### `Cell::Sand`
- **Purpose:** Permanent, passable terrain (beach, sea floor, ground)
- **Simulation behaviour:** Treated as `Air` — water flows through freely
- **Visual:** Flat low-profile mesh (roughly 20% of full cell height), sandy tan (`#d4aa6a`)
- **Player interaction:** Cannot be placed or erased — level geometry only

Neither cell type is available as a toolbar tool. They exist only in level files.

## Level File Format

Level layouts are defined in JSON files under `levels/` in the project root:

```
levels/
  coastal-bowl.json
  harbour-inlet.json    ← placeholder for future level
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
- Cell values match the `Cell` enum variants: `"Rock"`, `"Sand"`, `"Air"`, `{ "Water": 0.0–1.0 }`, `{ "Object": weight_kg }`
- `default_inlet_mode` sets the initial `WaterInletMode` when the level loads

### Coastal Bowl Layout

The `coastal-bowl.json` level defines:

- **Rock rim** — Rock cells along the entire left column, right column, and top row, with a single gap at the top-centre for the water inlet
- **Sand beach** — A 3-row band of Sand cells along the bottom, tapering up ~5 columns on the left side to suggest a shoreline
- **Ocean inlet** — Bottom-left corner (~6×6 cells) pre-filled with `Water(1.0)` representing the open sea
- **Rock outcrops** — 3–4 small clusters of Rock cells (2–4 cells each) scattered in the bowl interior to break up flat ground and redirect water
- **Open centre** — Remaining interior cells are Air — the town area where the player places buildings

## Level Loading (`src/levels.rs`)

A new `LevelsPlugin` is responsible for:

- Defining a `CurrentLevel` resource that holds the loaded `LevelData` struct
- `load_level(path)` — reads a JSON file, deserialises it, writes cells into the `Grid` resource, sets inlet mode on `GameState`
- `setup_level` system (Startup) — loads `levels/coastal-bowl.json` on app start
- Reset button triggers a re-run of `load_level` with the current level path (replaces the existing blank-grid reset)

This module is separate from `persistence.rs`, which handles player save/load. Save/load continues to serialise the full live cell state and is unaffected by level loading.

## Visual Theming

### Colour Palette (Warm Mediterranean)

| Element | Colour | Hex |
|---------|--------|-----|
| Sky (top) | Bright blue | `#5ba3d9` |
| Sky (horizon) | Pale haze | `#d4ecf7` |
| Rock | Stone grey-brown | `#7a6a5a` |
| Sand | Warm tan | `#d4aa6a` |
| Water (full) | Mediterranean blue | `#3a7fc1` |
| Water (froth) | Existing froth texture | — |

### Sky Background

A single large flat `Plane` mesh placed behind the grid (negative Z, outside camera clip) with a `StandardMaterial` using a vertical gradient via a programmatic texture (top: `#5ba3d9`, bottom: `#d4ecf7`). Sized to fill the visible camera frustum. Created in `render.rs` alongside the existing tile setup.

### Material Palette

`Rock` and `Sand` materials are added to the existing `MaterialPalette` resource in `render.rs`. The `render_grid` system already switches materials per cell type — it just needs the two new handles added.

## Out of Scope

- Player-placeable Sand or Rock tools
- Erosion mechanics (Sand eroding under water pressure)
- Animated sky or weather effects
- Level select UI (future work — the JSON infrastructure enables it)
- Harbour Inlet level content (file stub only)
