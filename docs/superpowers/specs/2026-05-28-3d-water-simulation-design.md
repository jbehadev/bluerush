# 3D Water Simulation — Design Spec

**Date:** 2026-05-28  
**Project:** BlueRush  
**Status:** Approved

---

## Overview

Convert BlueRush from a 2D grid simulation (viewed isometrically) to a true 3D voxel cellular automata simulation. Water flows in all 6 directions (±X, ±Y, ±Z) with gravity bias. The player edits the grid using a layer-by-layer slice editor. The renderer builds face-culled meshes for performance.

---

## Section 1: Core Data Model

### Axis Convention

```
          Y (height / gravity)
          ↑
          │         ╱ Z (depth, front→back)
          │       ╱
          │     ╱
          └──────────── X (left/right)
```

- Water falls in the **−Y direction** (gravity)
- Terrain is laid out on the **XZ plane** (horizontal)
- The layer slider moves through **Y slices** (horizontal cuts at a given height)
- Camera orbits above looking down at the XZ landscape

### Grid3D Struct

```rust
// src/simulation3d.rs
pub struct Grid3D {
    pub width:  usize,  // X axis
    pub height: usize,  // Y axis (vertical, gravity)
    pub depth:  usize,  // Z axis
    pub cells:  Vec<Cell>,
}

fn idx(&self, x: usize, y: usize, z: usize) -> usize {
    y * self.width * self.depth + z * self.width + x
}
```

**Starting grid size:** 40 × 20 × 40 = 32,000 cells (~16× current 2D grid).

### Cell Enum

The existing `Cell` enum (`Air, Water(f32), Object(f32), Wall, Spring, Drain, Rock, Sand, Building`) is **reused as-is** from `simulation.rs`. No new variants are needed — all 3D behaviour comes from how cells flow through the 3D grid.

### What changes vs what stays the same

| Component | Status | Notes |
|---|---|---|
| `Cell` enum | Unchanged | Reused from `simulation.rs` |
| `MAX_WATER_KG` constant | Unchanged | Same physics constants |
| `Grid` struct | Replaced | `Grid3D` replaces `Grid` in active use |
| `step_simulation` | New | `step_simulation_3d` — 6 neighbors |
| `step_objects` | New | `step_objects_3d` — same MoveIntent pattern |
| `build_depth_pressure` | New | Scans Y columns over XZ plane |

---

## Section 2: Simulation Algorithm

### 6-Direction Flow

```rust
// Gravity-first neighbor ordering in step_simulation_3d:
// 1. Try Y-down first (gravity priority)
// 2. Spread remaining laterally (X±, Z± equally)
// 3. Y-up only under pressure (same as current 2D)

let neighbors = [
    (x,   y-1, z  ),  // down  (gravity — evaluated first)
    (x-1, y,   z  ),  // left
    (x+1, y,   z  ),  // right
    (x,   y,   z-1),  // front
    (x,   y,   z+1),  // back
    (x,   y+1, z  ),  // up (pressure only)
];
```

### Delta Buffer Pattern

The existing conservation-safe delta buffer pattern is unchanged: accumulate all water transfers into a `Vec<f32> delta` buffer, then apply all at once. This prevents the water-creation bug (multiple cells writing to the same neighbor) that was caught by unit tests in Session 3.

### Depth Pressure in 3D

```rust
// build_depth_pressure_3d: scan each (x,z) column top→bottom
for z in 0..depth {
    for x in 0..width {
        let mut accum = 0.0;
        for y in (0..height).rev() {  // top→bottom
            accum += water_at(x, y, z);
            pressure[idx(x, y, z)] = accum;
        }
    }
}
```

### Object Movement — MoveIntent in 3D

`step_objects_3d` uses the same 3-pass MoveIntent pattern (collect → detect conflicts → apply) but considers 6 neighbors for pressure force vectors. An object moves if the net force across all 6 axes exceeds its weight threshold. The anti-oscillation deadzone applies on the X and Z axes only — gravity is always decisive on the Y axis.

### Unit Tests

All existing unit tests continue to pass against `simulation.rs` (unchanged). New tests in `simulation3d.rs` cover:
- Water conservation in 3D (no mass created or destroyed)
- Gravity flows water down before lateral spread
- Depth pressure accumulates correctly along Y columns
- Objects do not move below weight threshold

---

## Section 3: Rendering

### Face Culling

With 32,000 voxels, spawning one full cube per cell is not viable — most faces are hidden inside solid terrain. The renderer builds a single mesh per material type, emitting only the faces adjacent to Air (or transparent) neighbors.

```
for each voxel (x, y, z) that is not Air:
  for each of 6 faces:
    if neighbor in that direction is Air:
      → emit 2 triangles for that face
    else:
      → skip (hidden face)
```

The mesh is rebuilt whenever the grid changes (not every frame — rebuilding 32k voxels per frame would be too slow). A `GridDirty` event signals the render system to rebuild. A single `Mesh` entity per material type replaces the current one-entity-per-cell approach, drastically reducing draw calls. Typical terrain exposes ~5–10% of faces, keeping the total face count comparable to the current 2D render.

### Camera — Orbital Top-Down

Replace the fixed isometric orthographic camera with a perspective orbital camera centred on the grid:

- **Middle-click drag** → orbit (rotate around grid centre)
- **Scroll wheel** → zoom in/out
- **Right-click drag** → pan
- Default angle: ~45° elevation, looking down at the XZ landscape

Uses Bevy's existing `Camera3d` with perspective projection — same component as current, new controls.

### Layer Editor Visualization

The active Y layer (selected via slider) is highlighted with a semi-transparent plane gizmo at that height. Voxels above the active layer render at reduced opacity so you can see the slice you're editing. Voxels at or below the active layer render at full opacity. Uses the existing `Gizmos` infrastructure.

### MaterialPalette

The existing `MaterialPalette` resource (water gradient, rock, sand, objects, heatmap) is **reused unchanged**. Each face in the mesh gets the material matching its cell type.

---

## Section 4: Layer Editor UI & Level Format

### Layer Editor UI

The existing bottom panel gains a **Layer control** alongside Speed and Brush:

- **Layer: ▼ 8 ▲** — decrement/increment the active Y layer (0 = ground floor, height-1 = top)
- Active layer shown in status bar: `"Layer 8 / 20"`
- Click-to-place works exactly as before — places into the active Y layer at the (X,Z) position under the cursor
- Hover cursor gizmo projects onto the active layer plane

```rust
// New resource in grid.rs
#[derive(Resource)]
pub struct ActiveLayer { pub y: usize }
```

Uses the same `Button`, `Interaction`, and label update patterns from Sessions 7–10. No new UI framework concepts.

### Level Format — Extended to 3D

The existing JSON level format adds `depth` and a `z` coordinate to each cell placement. Old 2D level files remain loadable — missing `depth` defaults to `1`, missing `z` defaults to `0`.

```json
// Before (2D)
{ "name": "coastal-bowl", "width": 60, "height": 33,
  "cells": [{"x": 0, "y": 0, "cell": "Wall"}] }

// After (3D)
{ "name": "valley-flood", "width": 40, "height": 20, "depth": 40,
  "cells": [{"x": 0, "y": 0, "z": 0, "cell": "Rock"}] }
```

`LevelData` gains `depth: Option<usize>` and `CellPlacement` gains `z: Option<usize>` — both optional for backwards compatibility.

### First 3D Level: "Valley Flood"

A 40×20×40 bowl-shaped valley:
- Rock walls on all 4 XZ edges
- Rock floor at Y=0
- Terrain sloping up toward the edges
- A Spring in one corner at Y=1 releases water that flows downhill and pools in the valley centre

---

## Section 5: Migration Plan

### File Changes

| File | Status | What changes |
|---|---|---|
| `simulation3d.rs` | **New** | Grid3D, step_simulation_3d, step_objects_3d, build_depth_pressure_3d |
| `render.rs` | **Rewritten** | Face-culled mesh builder; orbital camera |
| `grid.rs` | **Modified** | ActiveLayer resource; handle_input uses Grid3D + active layer |
| `ui.rs` | **Modified** | Layer ▼/▲ buttons and label |
| `levels.rs` | **Modified** | depth/z fields (Option); valley-flood.json |
| `main.rs` | **Modified** | Register simulation3d; swap Grid → Grid3D |
| `simulation.rs` | **Kept** | Unchanged — all existing unit tests continue to pass |
| `textures.rs` / `persistence.rs` / `camera.rs` | **Kept** | Mostly unchanged; persistence extended for Grid3D |

### Implementation Order

Each step keeps the game compilable:

1. **Write `simulation3d.rs`** — Grid3D + pure functions + unit tests. No Bevy yet. _Game still runs in 2D._
2. **Update `grid.rs`** — swap to Grid3D internally; add ActiveLayer resource. _Game still renders._
3. **Extend `levels.rs`** — add depth/z fields; author valley-flood.json. _Level loads into Grid3D._
4. **Rewrite `render.rs`** — face-culled mesh + orbital camera. _Game now renders in true 3D._
5. **Update `ui.rs`** — add Layer ▼/▲ buttons. _Feature complete._
6. **Extend `persistence.rs`** — Grid3D save/load. _Save/load restored._

### What You'll Learn

3D array indexing, voxel face culling, custom mesh building in Bevy, orbital camera controls, and extending an existing ECS resource pattern with a new dimension.
