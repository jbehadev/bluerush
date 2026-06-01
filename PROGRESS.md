# BlueRush - Progress Log

## Game Concept
A flood simulation game where water rushes in and carries/destroys objects in its path. Grid/tile-based. Player places objects and directs water; heavy objects block or redirect water, light objects get carried away.

## Concepts Learned

### Session 1 & 2
- **ECS (Entity Component System)** — Bevy's core pattern
  - Entity: a unique ID created by `commands.spawn(...)`
  - Component: data attached to an entity (e.g. `Sprite`, `Transform`)
  - System: a function that queries and operates on entities
- **Marker components** — empty structs with `#[derive(Component)]` used to tag entities for query filtering (`With<MyBox>`)
- **Schedules** — `Startup` runs once, `Update` runs every frame
- **Resources** — global singletons accessed via `Res<T>` (e.g. `Res<Time>`)
- **Delta time** — `time.delta_secs()` for frame-rate independent movement
- **Query filtering** — `Query<&mut Transform, With<MyBox>>`
- **Plugins** — structs implementing `Plugin` trait to organize systems/resources; registered with `.add_plugins(...)`
- **Modules** — `mod grid;` in `main.rs` to split code into separate files (`src/grid.rs`)
- **Enums with data** — `Cell` enum with variants `Air`, `Water`, `Object(f32)` where `Object` carries a weight value
- **Flat Vec grid indexing** — `y * width + x` for row-major 2D grids
- **`set_cell` helper** — encapsulates index math behind a clean method
- **z-layering** — `Transform::from_xyz(..., z)` controls draw order
- **Type casting** — `usize as f32` for converting grid indices to positions
- **`.into()` for type conversion** — e.g. `Srgba` → `Color`

### Session 3
- **Enums with data (extended)** — `Cell::Water(f32)` carries fill level 0.0–1.0
- **Color mixing** — `Color::srgb(1.0 - fill, 1.0 - fill, 1.0)` blends white→blue based on fill; alpha not needed
- **`usize` underflow** — unsigned integers can't go negative; starting loops at `y=1` makes `y-1` safe
- **`isize`** — signed integer type used for direction offsets where negative values are needed
- **Resources for game state** — `GameState { water_flow: bool }` as a `#[derive(Resource)]` struct
- **`ResMut<T>`** — mutable resource access; `*resource = value` dereferences to replace the whole value
- **Keyboard input** — `Res<ButtonInput<KeyCode>>` with `.just_pressed()` vs `.pressed()`
- **Boolean toggle** — `state.water_flow = !state.water_flow`
- **`matches!` macro** — concise pattern matching returning bool: `matches!(cell, Cell::Water(_))`
- **Unit tests in Rust** — `#[cfg(test)]` module with `#[test]` functions; lives in same file to access private types
- **`assert!` and `assert_eq!`** — test assertion macros
- **Delta buffer pattern** — accumulate changes in a separate `Vec<f32>`, apply all at once to prevent multiple writes creating water from nothing
- **`.clamp(min, max)`** — clamps a value to a range, equivalent to `.max(min).min(max)`
- **Water conservation** — a simulation bug where multiple cells writing to the same neighbor created water; caught by a unit test

### Session 4
- **`.just_pressed()` vs `.pressed()`** — `pressed` fires every frame the key is held; `just_pressed` fires only on the first frame
- **`#[derive(Debug)]`** — enables `{:?}` formatting for custom types; needed for `println!` debugging
- **`get_cell` helper** — added to `Grid` impl for read-only cell access
- **Right-click debug tooltip** — `mouse.just_pressed(MouseButton::Right)` to inspect cell state via `println!`
- **Pressure-based object movement** — `step_objects` function uses water pressure differences to push objects
- **MoveIntent pattern** — collect all intended moves first (Pass 1), detect conflicts (Pass 2), apply conflict-free moves (Pass 3)
- **Swap bug** — when object A moves to B's src cell while B moves away, A gets overwritten; fixed with `dst_counts[intent.src] > 0` check
- **`HashSet<usize>`** — considered but not needed; `dst_counts` vec was sufficient for conflict detection
- **Object-water swap** — vacated cell gets the water that was at the destination (`grid.cells[intent.dst].clone()`), conserving water mass
- **Threshold-based direction** — `x_force.abs() > threshold` decides if force is strong enough to trigger movement on each axis independently
- **force_kg vs weight** — `force_kg = pressure_diff * 1000.0`; object only moves if `force_kg > weight`

### Session 5
- **Depth-based pressure** — `build_depth_pressure` scans each column top-down, accumulating water fill levels; cells deeper in the column get higher pressure values
- **Ocean floor base pressure** — `y=0` is hardcoded to `2000.0` representing infinite water pressure behind the bottom row; this ensures objects near the floor are always pushed upward
- **Why fill-level pressure fails in full grids** — when water equalises everywhere (all cells ~0.95 fill), pressure differences between neighbors approach zero; depth pressure avoids this by encoding column height, not just local fill
- **Test layout matters** — tests for depth pressure must be aware that depth accumulates from above; a single water cell at y=0 has depth=0 (nothing above it)
- **Dead code warnings** — unused struct fields trigger `#[warn(dead_code)]`

### Session 6
- **`src/textures.rs` module** — new file with `TexturesPlugin` and `TextureAssets` resource holding `Handle<Image>` fields
- **Programmatic textures** — `Image::new(Extent3d, TextureDimension::D2, Vec<u8>, TextureFormat, RenderAssetUsages)` creates a texture from raw RGBA bytes at startup
- **`sprite.image` vs `sprite.color`** — image sets the texture; color is a tint on top; `Handle::default()` clears the texture back to a plain color rectangle
- **`assets.add(image)`** — inserts an `Image` into Bevy's asset storage, returns a `Handle<Image>`
- **`rand` crate** — added as dependency; `rand::thread_rng()` + `rng.r#gen::<f32>()` for random values
- **`r#gen` raw identifier** — `gen` became a reserved keyword in Rust 2024 edition; `r#gen` escapes it to use the `rand 0.8` method
- **`crate::textures::TextureAssets`** — cross-module import; modules can't see each other without explicit `use` paths
- **Froth rendering** — low fill water (`fill < 0.1`) uses a programmatic speckled texture; higher fill uses color gradient
- **`HashMap<usize, Vec<usize>>`** — used in Pass 2 of MoveIntent to group intents by destination
- **Random conflict resolution** — when multiple objects want the same cell, one is chosen randomly via `candidates[rng.r#gen::<usize>() % candidates.len()]`
- **`HashSet<usize>` for winners** — tracks which intent indices won; used to skip moves whose src is another winner's dst

## What Was Built
- `Cell::Water(f32)` fill level with color gradient rendering
- Border walls on left, right, and top edges using `Object(9999.0)`
- `flow_water` system — fills bottom row each tick when enabled
- `simulate_flow` system — pressure-based diffusion using delta buffer; water spreads in all four directions equally
- `GameState` resource — `X` key toggles water flow, `R` key resets grid
- `step_simulation` pure function — simulation logic extracted from Bevy system for testability
- `step_objects` pure function — moves objects based on water pressure; 3-pass MoveIntent with random conflict resolution
- `build_depth_pressure` pure function — computes per-column cumulative depth pressure table; y=0 hardcoded to 2000.0
- `src/textures.rs` — `TexturesPlugin`, `TextureAssets` resource, programmatic froth texture
- Froth rendering — low fill water cells show a speckled white/blue texture
- Right-click cell inspector — prints cell type and value to console
- Six unit tests (5 passing, 1 failing — see below)

## Current File Structure
```
src/
  main.rs      — app setup, registers TexturesPlugin + GridPlugin
  grid.rs      — GridPlugin, Cell/Grid/Tile/GameState types, all systems and simulation logic
  textures.rs  — TexturesPlugin, TextureAssets resource, make_froth_frame()
```

### Session 7
- **`PANEL_HEIGHT` constant** — reserves pixel space at bottom of window for UI; `OFFSET_Y` shifted up by `PANEL_HEIGHT` so the grid doesn't overlap the panel
- **Grid height shrinkage** — `height = (WINDOW_HEIGHT - PANEL_HEIGHT) / TILE_SIZE`; grid area is now 540px tall (33 tiles) instead of 600px (37 tiles)
- **Bevy UI nodes** — `Node { position_type: PositionType::Absolute, bottom: Val::Px(0.0), ... }` for absolute panel placement
- **`Button` component** — marks an entity as a UI button; Bevy automatically adds `Interaction` and tracks Hover/Pressed/None states
- **`Changed<Interaction>` filter** — `Query<..., Changed<Interaction>>` only runs the system on entities whose interaction state changed this frame; efficient for button handling
- **`SelectedWeight` resource** — holds the currently chosen object weight (200/500/1000 kg); updated by `handle_weight_buttons`
- **`WeightButton(f32)` component** — data component on each button entity storing its associated weight value
- **`selected.is_changed()`** — skips `update_button_colors` entirely when the selection hasn't changed; avoids redundant work each frame
- **Panel click guard** — `handle_input` returns early if `world_y < -(WINDOW_HEIGHT/2) + PANEL_HEIGHT` so clicking buttons doesn't accidentally place objects
- **Right-click bounds check** — added `grid_x < grid.width && grid_y < grid.height` guard to right-click debug path (was missing before)

## What Was Built
- `Cell::Water(f32)` fill level with color gradient rendering
- Border walls on left, right, and top edges using `Object(9999.0)`
- `flow_water` system — fills bottom row each tick when enabled
- `simulate_flow` system — pressure-based diffusion using delta buffer; water spreads in all four directions equally
- `GameState` resource — `X` key toggles water flow, `R` key resets grid
- `step_simulation` pure function — simulation logic extracted from Bevy system for testability
- `step_objects` pure function — moves objects based on water pressure; 3-pass MoveIntent with random conflict resolution
- `build_depth_pressure` pure function — computes per-column cumulative depth pressure table; y=0 hardcoded to 2000.0
- `src/textures.rs` — `TexturesPlugin`, `TextureAssets` resource, programmatic froth texture
- Froth rendering — low fill water cells show a speckled white/blue texture
- Right-click cell inspector — prints cell type and value to console
- **Bottom UI panel** — dark bar with 200 kg / 500 kg / 1000 kg buttons; selected button highlighted in blue
- **`SelectedWeight` resource** — tracks active weight; `handle_input` uses it when placing objects

### Session 8–10 (3D Rendering Overhaul)
- **3D isometric view** — replaced 2D sprite rendering with 3D cubes (`Cuboid`, `Mesh3d`, `MeshMaterial3d`) viewed from an orthographic isometric camera (`Camera3d` + `OrthographicProjection`)
- **Material palette** — `MaterialPalette` resource pre-creates `StandardMaterial` handles for all cell types (air, wall, spring, water gradient, object weights, heatmap); enables draw-call batching
- **Dynamic tile height** — `render_grid` sets `transform.scale.y` and `transform.translation.y` based on cell type; water height reflects fill level, walls/springs are full height
- **Camera controls** — mouse scroll to zoom, middle-click drag to pan; camera stays focused on grid center with adjustable offset
- **Heatmap 3D rendering** — `render_heat_grid_3d` uses a rainbow pressure color ramp with distinct material palette
- **Anti-oscillation physics** — horizontal deadzone in `step_objects` prevents objects from jittering left-right when pressure is nearly balanced (requires >10% imbalance to move)
- **Brush size labels** — `SpeedLabel` and `BrushLabel` text components update dynamically
- **Hover cursor gizmo** — `draw_hover_cursor` system uses Bevy `Gizmos` to draw yellow wireframe rectangles over hovered cells; respects brush radius; `depth_bias: -1.0` ensures visibility over all geometry
- **Save/load persistence** — `Cmd+S` / `Cmd+O` with native file dialogs via `rfd` crate; grid serialized/deserialized with `serde`

## Current File Structure
```
src/
  main.rs        — app setup, registers TexturesPlugin + GridPlugin
  grid.rs        — GridPlugin, all UI/rendering/input systems, 3D camera, gizmos
  simulation.rs  — Cell/Grid types, step_simulation, step_objects, build_depth_pressure
  textures.rs    — TexturesPlugin, TextureAssets resource, programmatic froth textures
  persistence.rs — save/load grid to JSON files
```

## Where We Left Off
3D isometric rendering is fully working with material palette, camera controls, hover cursor gizmo, and save/load. Anti-oscillation fix applied to object physics.

## System Schedule

### Startup
| System | Plugin | Purpose |
|--------|--------|---------|
| `setup` | GridPlugin | Insert `GameState`, `ViewMode`, `SelectedTool`, `Grid` resources |
| `setup_camera` | CameraPlugin | Spawn 3D isometric `Camera3d` |
| `setup_ui` | UiPlugin | Spawn all UI panel nodes and buttons |
| `setup_render` | RenderPlugin | Spawn tile mesh entities, build `MaterialPalette` |

### Update (GridPlugin)
| System | Purpose |
|--------|---------|
| `simulate_objects` | Run `step_objects` × `sim_speed`; skipped when flow off |
| `flow_water` | Fill top row with water at `MAX_WATER_KG` per tick |
| `simulate_flow` | Run `step_simulation` × `sim_speed` (pressure diffusion) |
| `handle_input` | Mouse place/erase, keyboard shortcuts, undo/redo, view toggle |
| `animate_gate` | Open/close wall gate at top center one cell per frame |
| `handle_save` / `handle_load` | Write `SaveRequested` / `LoadRequested` messages → `PendingFileOp` |
| `poll_file_op` | Poll async file dialog thread; apply loaded grid |

### Update (CameraPlugin)
| `camera_controls` | Scroll-to-zoom, middle-click pan |

### Update (UiPlugin)
| `handle_weight_buttons` / `handle_eraser_button` / `handle_spring_button` / `handle_drain_button` | Tool selection buttons |
| `update_tool_buttons` | Highlight active tool button |
| `handle_inlet_toggle` / `update_inlet_button` | Flow on/off button |
| `handle_view_toggle` / `update_view_buttons` | Normal / Pressure / FlowArrows toggle |
| `handle_reset` | Reset grid button |
| `handle_speed_buttons` / `update_speed_label` | Sim speed ±  |
| `handle_brush_buttons` / `update_brush_label` | Brush radius ± |
| `update_status` | Status bar text (fps, cell count) |

### Update (RenderPlugin)
| `render_grid` | Update tile transforms/materials from `Grid` each frame |
| `draw_hover_cursor` | Yellow gizmo wireframe over hovered cells |
| `draw_flow_arrows` | Gizmo arrows showing water flow direction (FlowArrows mode) |

### Session 11 (Coastal Environment)
- **`Cell::Rock` and `Cell::Sand`** — two new level-geometry cell types. Rock is impassable (like Wall), Sand is passable (like Air). Both are level-only — not player-placeable. Added to all exhaustive match arms across `simulation.rs`, `grid.rs`, `render.rs`.
- **`Grid::blank(width, height)`** — new constructor returning an all-Air grid with no hardcoded walls or reservoir. Used by the level loader as the starting state before applying sparse cell placements.
- **`serde` on `InletMode`** — added `Serialize, Deserialize` derives so inlet mode can round-trip through JSON level files.
- **`src/levels.rs` module** — `LevelsPlugin`, `CurrentLevel` resource, `LevelData` / `CellPlacement` structs, `pub fn load_level`. Loads a JSON level file at startup; falls back to `Grid::init` on error. System ordered `.after(crate::grid::setup)`.
- **JSON level format** — sparse cell placement format: `{ "name", "width", "height", "default_inlet_mode", "cells": [{"x","y","cell"}] }`. Cell values use serde enum serialisation.
- **`levels/coastal-bowl.json`** — 60×33 grid: Rock rim on all 4 edges (4-cell inlet gap at top-centre x=28–31), Sand beach along the bottom 3 rows tapering on the left, Water ocean inlet in the bottom-left 6×6 corner, 3 Rock outcrops in the interior.
- **`levels/harbour-inlet.json`** — minimal valid stub (all-Air, same dimensions) for future authoring.
- **Reset reloads level** — both the Reset button (`handle_reset` in `ui.rs`) and `R` key shortcut (`handle_input` in `grid.rs`) now call `load_level` instead of `Grid::init`.
- **Mediterranean visual theme** — Rock rendered in stone grey-brown (`#7a6a5a`), Sand in warm tan (`#d4aa6a`), sky background `ClearColor` set to Mediterranean blue (`#5ba3d9`).
- **Placement guard** — `Cell::Rock` and `Cell::Sand` added to the placement guard in `grid.rs` so players cannot overwrite level terrain.
- **17/17 unit tests pass** — 3 new tests: `test_grid_blank_is_all_air`, `test_rock_blocks_water_flow`, `test_sand_allows_water_flow`.

## Current File Structure
```
src/
  main.rs        — app setup, registers all plugins
  grid.rs        — GridPlugin, all UI/rendering/input systems, 3D camera, gizmos
  simulation.rs  — Cell/Grid types (inc. Rock/Sand), step_simulation, step_objects, build_depth_pressure, Grid::blank
  textures.rs    — TexturesPlugin, TextureAssets resource, programmatic froth textures
  persistence.rs — save/load grid to JSON files
  levels.rs      — LevelsPlugin, CurrentLevel resource, load_level, LevelData/CellPlacement
  render.rs      — RenderPlugin, MaterialPalette (inc. rock/sand), tile rendering, camera, gizmos
levels/
  coastal-bowl.json  — Coastal Bowl level (60×33, Rock rim, Sand beach, Water inlet)
  harbour-inlet.json — Harbour Inlet stub (all-Air, 60×33)
```

## Where We Left Off

Coastal environment fully implemented and on branch `feat/coastal-environment`. PR ready to merge.

---

### Session 12–13 (3D voxel experiment — parked)

Explored converting the sim to a true 3D voxel simulation on branch
`feat/3d-simulation` (`Grid3D`, face-culled cube meshes, pressure-based water,
layer-by-layer editing, column-fill placement tools). It worked, but the voxel look
was blocky and "didn't look better than the 2D original" for a lot of added machinery.
Parked as a reference — not the direction.

### Session 14 (Pivot: heightfield water surface)

- **Decision** — after a design comparison (voxel-mesh vs heightfield vs particle/SPH
  vs improved-voxels), pivoted to a **2.5D heightfield water surface**, branching off
  `main`. A quick particle spike (`src/bin/particle_demo.rs`, since removed) confirmed
  bare particles look like "ping pong balls" — water quality comes from a connected
  *surface*, not particle motion.
- **`src/bin/heightfield_demo.rs`** — proof-of-look spike (approved by the user): a
  single deforming surface mesh (W×D grid of vertices) whose height is driven by a
  **wave-equation** ripple sim; rendered as a translucent, low-roughness
  `StandardMaterial` over a sandy floor with a directional light for sheen. Reads as
  real water with **no custom shader**. Hold LMB to ripple, R to calm.
- **Concepts learned** — wave equation (height + velocity, neighbour-average
  Laplacian); building and mutating a `Mesh` each frame
  (`ATTRIBUTE_POSITION` / `ATTRIBUTE_NORMAL` via `Assets::get_mut`); slope-based vertex
  normals for lighting; `AlphaMode::Blend` translucency + low `perceptual_roughness`
  for sheen; 3D camera ray → plane intersection for mouse picking; extra binaries via
  `src/bin/` + `default-run` in Cargo.toml.

### Session 15 (Flooding + weighted objects)

- **`src/bin/flood_demo.rs`** — terrain + shallow-water flooding + weighted objects.
  - **Terrain** — a downhill CHANNEL (floor slopes back→front, U-shaped side walls),
    built once as a static mesh; water runs down it.
  - **Flow** — mass-conserving "water finds its level" relaxation: each column sends
    water to lower-surface neighbours (capped by available depth), so water flows
    downhill and pools to a flat level. A source feeds the top; the front edge drains,
    sustaining a current. A small visual ripple layer rides on top for liveliness.
  - **Current field** — `step_flow` accumulates a per-cell `flow: Vec<Vec2>` (net water
    movement). Floating objects drift toward `flow × FLOW_TO_SPEED × mobility`, so they
    follow where the water is actually moving — not just the surface slope (which is
    weak and vanishes in still/deep water, the reason an earlier surface-gradient push
    barely moved things).
  - **Weighted objects** (`FloatObject`) — float once the water can support them
    (`depth × BUOYANCY ≥ weight`) and are carried by the current scaled by
    `mobility = REF_WEIGHT / weight`: light blocks ride the flood, heavy ones resist.
  - **Controls** — LMB pour, RMB drop a light object, R drain.
- **Concepts** — deriving a per-cell current vector from transfer directions; buoyancy +
  weight-based mobility; rebuilding a dynamic index buffer to clip the water mesh to a
  clean waterline; Bevy `Mut` deref vs the borrow checker (use a temp local for
  `a += a.something()` / `a = a.lerp(...)`).

### Session 16 (Collision + two-way coupling)

- **Object–object collision** (`object_collision`) — mass-weighted separation: overlapping
  objects push apart, the lighter one moving more, so a heavy block holds its ground and
  others pile against it. O(n²) over the (small) object set, applied via a per-Entity
  correction map.
- **Two-way coupling / damming** (`build_obstacles` + `Obstacle` resource) — a *grounded*
  object (too heavy to float at its current depth) raises an effective floor under its
  footprint; `step_flow` uses `terrain + obstacle` as the floor, so water **backs up behind
  it and diverts around** it. Floating objects don't obstruct (they ride on top), which
  keeps it physical.
- Emergent result the user liked: the heavy block dams the channel, water splits around it,
  and the diverted current carries the lighter blocks past.

### Session 17 (Integration into the main app + controls)

- **Promoted the prototype to the real app.** `src/bin/flood_demo.rs` → `src/flood.rs` as a
  `FloodPlugin`; `main.rs` is now slim (load config, window + 60fps, `add_plugins(FloodPlugin)`).
  `cargo run` (the BlueRush binary) launches the heightfield flood game.
- **Retired the old 2D-voxel code** (the abandoned direction): deleted `simulation.rs`,
  `render.rs`, `grid.rs`, `ui.rs`, `levels.rs`, `persistence.rs`, `undo.rs`, and the demo bins.
  `camera.rs` / `textures.rs` left on disk, unreferenced (not in the module tree, so not
  compiled). Kept a single-file `flood.rs` to avoid cross-module privacy churn (per the
  integration-plan workflow's skeptic).
- **UI panel** (`setup_ui` + handlers): OBJECTS weight buttons (200–5000 kg), a **Pour Water**
  tool, and **WAVE** patterns — **Flood** (steady), **Sine** (pulsing), **Random** (gusty) —
  modulating the source via `run_source`. Left-click applies the selected tool; clicks over
  the panel are guarded by `cursor.x < PANEL_WIDTH`.
- **Weight-scaled blocks** — `obj_footprint` / `obj_height` scale a block's size with weight
  (sqrt-spaced); the unit cube mesh is scaled per object. Dam height + obstacle footprint are
  tied to the block's size, so heavy blocks are big, tall, and dam more; collision spacing
  scales too.
- **Orbit camera** (`OrbitCamera` + `camera_controls`) — right-drag orbit, middle-drag pan,
  scroll zoom (gentle, clamped), via `AccumulatedMouseMotion` / `AccumulatedMouseScroll`.
- **Placement indicator** (`draw_placement_cursor`) — a gizmo wireframe box at the cursor
  sized to the selected weight (flat square for Pour) showing where/how big the next drop lands.
- **Pause** (`Paused` + `toggle_pause` / Pause button) — Space or the top button freezes the
  water + object sim (the five sim systems early-return) while camera, placement, and rendering
  keep running.
- **Tried + reverted for overtopping:** foam-by-flow-speed (washed out) and rendering the water
  surface up the obstacle floor (tented over blocks). Both looked bad; reverted to the clean
  flat surface. Proper overtopping cresting needs a **weir-model** rework (see What Comes Next).
- Note: deleting `simulation.rs` removed the old 2D unit tests; the flood game has no unit
  tests yet (pure-function extraction + tests is a future cleanup).

## Current File Structure (heightfield game)
```
src/
  main.rs    — window/config + add_plugins(FloodPlugin)
  flood.rs   — the whole game: terrain, water sim, objects, render, UI, camera (single module)
  config.rs  — AppConfig (still old fields; adapt later)
  camera.rs, textures.rs — unreferenced (old, kept for reference)
```

### Session 18 (Erase, height/depth gradients, docs)

- **Erase tool** — panel button; with it selected, left-click deletes the nearest block to the
  cursor (`handle_click` Erase arm, `handle_erase_button`, `EraseButton`); works while paused.
- **Terrain height gradient** — `build_terrain_mesh` writes per-vertex colours lerping light
  brown (low streambed) → green (high banks) over `0..SLOPE_HEIGHT+BANK_MAX`; terrain material
  base is white so the gradient shows. Makes elevation legible.
- **Water depth shading** — `update_water_mesh` writes per-vertex colours lerping shallow
  (light, clear) → deep (dark, more opaque) by `depth/DEPTH_COLOR_MAX`; water material base is
  white. Pools/dammed water read as deep, the flowing sheet as shallow.
- **`docs/WATER_FLOW.md`** — step-by-step explanation of the water sim + a new-methods reference.
- **Rendering note** — both terrain and water use the white-base-material + per-vertex
  `ATTRIBUTE_COLOR` trick (vertex colours are LINEAR; build via `Color::srgb(..).to_linear()`).

### Session 19 (Weir-model overtopping — water crests blocks)

- **Problem** — water wouldn't crest over a grounded block: the surface dipped to a dry hole *at*
  the block and surged *below* it ("goes down before, raises sharply after").
- **Two coupled causes found:**
  1. `step_flow` moved water on the *full* surface gap `si - sj`. A block raises its cell's floor
     by its whole height (the `Obstacle` field), so when water crested onto a block cell the
     ~full-height gap to the downstream cell dumped the entire column in one step — draining the
     crest to a dry dip and surging the cell below.
  2. `update_water_mesh` drew the surface at `terrain + depth`, ignoring the raised `obstacle`
     floor the flow sim used — so crest water was drawn at bare ground, not on the block top.
- **Fix — weir flux** (`step_flow`): flow is now driven by the head *above the sill* (the higher of
  the two cells' floors): `sill = max(floor_i, floor_j)`, `head = max(0, surface - sill)`, move
  `head_i - head_j`. A tall block dams until water backs up to its crest, then spills only the thin
  overtopping layer. No artificial floor-raise in the depth bookkeeping → depth reads from ground.
- **Fix — consistent render** (`update_water_mesh`): surface drawn at `terrain + obstacle + depth`,
  matching the flow floor, so crest water sits on the block as a continuous sheet. Dry obstacle
  cells stay below `WET` and aren't emitted (no water "tent" over an un-flooded block).
- **Fix — tight footprint** (`build_obstacles`): the old footprint was a cell-radius + 1 margin,
  ballooning to ~3× the cube (a big raised sandy shelf poking through the backed-up water). Now
  only cells whose centre lies under `obj_footprint` are raised — the dammed area matches the cube.
  Tradeoff: dropped the +1 gap-seal, so a row of blocks with a 1-cell gap can leak a thin stream;
  place blocks touching to dam as a solid wall.
- **Known remaining artifact (accepted):** the upstream crest cell renders its overtopping water as
  a flat sheet sticking ~1 cell (~6 wu) past the block's back face — a grid-resolution effect, not a
  bug. Left as-is; a clean fix needs sub-cell geometry near obstacles.
- **Concepts** — weir/spillway flux limiting (move only water above the shared sill); keeping the
  render floor consistent with the sim floor; matching a footprint mask to world-space extent
  instead of a rounded cell radius.

## Where We Left Off (current)

Weir-model overtopping done on `feat/heightfield-water`: water backs up behind a grounded block and
crests over it as a continuous sheet (lighter/shorter blocks crest readily; tall heavy blocks mostly
dam and divert, which is physical). The heightfield flood game IS the main app: `cargo run` launches a
meandering stream bed that floods; weighted blocks (sized by weight) are carried by the current,
collide/pile, and dam/divert the flow. UI for weights + wave patterns + Pour + Erase; orbit
camera; placement preview; pause (Space). Terrain is height-gradient shaded (brown→green) and
water is depth-shaded (shallow→deep). Documented in `docs/WATER_FLOW.md`.

Branch commits: `31b6831` look → `6b853e4` flooding+objects → `0d05b25` collision+damming →
`9f7c14f` main-app integration+UI+camera+pause → `b6579e3` erase+docs → (this) gradients.
Not yet pushed; old 2D code preserved on `main`.

## What Comes Next
- **Crest-edge polish (optional)** — the upstream overtopping sheet sticks out ~1 cell past the
  block back; needs finer geometry near obstacles if it ever bothers us.
- **Config + cleanup** — adapt `config.rs` fields to the heightfield game; drop now-unused deps
  (rand stays — used by Random wave; `serde_json`/`rfd` go); remove `camera.rs`/`textures.rs`/old levels.
- **Tests** — extract pure sim helpers (flow, buoyancy, obstacle) and unit-test them.
- **Object destruction** — a strong enough current sweeps away or breaks objects.
- **Levels** — author terrain heightmaps; save/load.
