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
  main.rs     — window/config + add_plugins(FloodPlugin, SceneryPlugin, RainPlugin)
  flood.rs    — terrain, water sim, objects, render, UI, camera; shared helpers are pub(crate)
  scenery.rs  — scenery sets (trees / city / ruins): pure placement + rebuild system + panel buttons
  rain.rs     — the source drawn as a rain cloud with falling rain (visual only)
  surround.rs — decorative land + distant mountains around the map (fills the horizon)
  config.rs   — AppConfig (window size + level path, from config.yaml)
  level.rs    — level yaml + PNG heightmap loading (incl. `scenery:` list)
assets/shaders/water.wesl   — custom water material
assets/shaders/terrain.wesl — ground cover (sand / grass / rock by height, slope, noise)
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

### Session 20 (PNG heightmap levels — terrain edited outside the code)

- **`src/level.rs`** — the level system. A level = `levels/<name>.yaml` (metadata) +
  `levels/<name>.png` (grayscale heightmap: black = low, white = high). `load` reads the yaml,
  decodes the PNG (`image::open(..).to_luma16()` — 8-bit widens automatically), and
  **bilinearly resamples** it onto the W×D grid scaled by `height_scale`. Any image editor is
  now the level editor. `write_template` does the reverse: bakes a heights array to a 16-bit
  PNG (normalised so the tallest point is white, `height_scale` records the real max) + yaml.
- **Map-fraction coordinates** — source/object positions in the yaml are 0..1 fractions of the
  map, so levels are independent of image size *and* grid resolution.
- **First-run template** — if `config.yaml`'s `level:` points at a missing file, `setup` bakes
  the built-in valley (`builtin_level()` — procedural `terrain_height` + default source +
  starter trio) out as an editable template, same discoverability pattern as `AppConfig::load`.
  A broken-but-present level file falls back to the built-in valley without overwriting it.
- **flood.rs integration** — `FloodPlugin { level }` carries the path from config;
  `Source { x, z, radius, rate }` resource (from the level) replaces the hardcoded
  `channel_center(4)` feed in `run_source`; `spawn_object` / `draw_placement_cursor` read
  heights from the `Terrain` resource instead of calling `terrain_height` directly; starter
  objects come from `level.objects`; terrain colour gradient now spans the *actual* height
  range so custom maps shade correctly.
- **`config.rs` trimmed** — `AppConfig` is now just `window_width/height` + `level`
  (`#[serde(default)]` so missing fields fall back); stale grid fields removed.
- **Shipped level** — `levels/valley.{yaml,png}` committed, regenerable via
  `cargo test generate_valley_level -- --ignored` (an `#[ignore]`d golden-file generator test).
- **Tests** — 4 passing: bilinear resample (identity + midpoint), template round-trip,
  builtin-valley round-trip (max height error < 0.01 through 16-bit quantisation).
- **Docs** — `docs/LEVELS.md`: format spec, image-editor workflow (front edge = drain, dark
  line = water path), how to make a new level, shipped-level table.
- **Three more shipped levels** (baked by the `generate_extra_levels` ignored test, all
  smoke-tested loading): **Winding River** (four tight S-bends, steep banks, a 3000 kg block
  grounded at a bend), **River Delta** (steep stem splits at z=0.4 into three smoothstepped
  distributaries over a flat fan — banks fade across the fan so spill spreads; 2500 kg block on
  the split), **Highland Lake** (paraboloid basin behind a gaussian ridge with a notch — lake
  rises to the sill and pours down a guided runout; 2000 kg block plugs the notch).
- **In-game level dropdown** — Bevy UI has no dropdown widget, so it's a button + collapsible
  list: `LevelLibrary` (scanned from `levels/*.yaml` at startup, labelled by each yaml's
  `name:`), a `LevelOptions` container toggled via `Display::None/Flex`, and a `PendingLevel`
  resource consumed by `switch_level`, which rebuilds the terrain mesh **in place**
  (`Assets::insert` on the kept `TerrainMesh` handle — returns a `Result` in 0.18), zeroes the
  water fields, despawns/respawns `FloatObject`s, and moves the `Source`. Startup is now
  `(setup, setup_ui).chain()` so the UI can read the library `setup` inserts (Bevy auto-inserts
  the command flush between chained systems).
- **Concepts** — `image` crate (decode to `Luma<u16>`, `ImageBuffer::save`); bilinear
  sampling; golden-file generation via an ignored test; plugin structs with config fields.

### Session 21 (Draggable walls)

- **Click vs drag** — with a weight selected, a click still drops a block; click-hold-drag-release
  builds a **wall** along the drag (drags shorter than one block footprint count as a click).
  `WallDrag` resource remembers the press point and the last cursor hit, so release works even
  off the map; press + release in the same frame (trackpad tap) still drops a block.
- **`Wall { a, b, weight }`** — static levee: never floats/moves, always dams. Thickness/height
  come from the selected weight. Rendered as child cube pieces (`with_children`), each sitting on
  the terrain under it so it hugs slopes; despawning the parent removes the pieces.
- **Integration** — `build_obstacles` raises cells inside the wall rectangle (dot/perp_dot
  projection); `object_collision` pushes blocks out of walls and cancels inward velocity; Erase
  removes the nearest wall; `switch_level` clears walls; gizmo outline previews the wall mid-drag.
- **Tests** — `closest_on_segment` helper unit-tested (6 passing + 3 ignored level generators).
- **Concepts** — parent/child entities, per-frame state in a resource, point-to-segment distance.

### Session 22 (Realistic water rendering)

- **Goal** — make the water look like real water (reference: a translucent green river reflecting
  the sky) instead of a flat tinted sheet.
- **Sky + sunlight** — camera gets `Atmosphere::earthlike` (physically based sky; needs a
  `ScatteringMedium` asset) + `AtmosphereEnvironmentMapLight` (lights the scene *and* gives the water
  a sky to reflect). Sun is `lux::RAW_SUNLIGHT` with shadows + a `CascadeShadowConfig` spanning the
  map; `Exposure { ev100: 14.2 }` + `Tonemapping::AcesFitted` + `Bloom::NATURAL` keep it from
  blowing out. `DepthPrepass` on the camera writes terrain/object depth before the water draws.
- **First custom shader** — `assets/shaders/water.wgsl`, plugged in via
  `ExtendedMaterial<StandardMaterial, WaterExt>` (`type WaterMaterial`, registered with
  `MaterialPlugin`). Bevy still does the lighting; the shader only changes its inputs:
  - **Depth colour** — thickness of water along the view ray = water depth − prepass depth;
    Beer–Lambert `1 − exp(−thickness·absorption)` blends clear shallow → murky teal deep.
  - **Flowing ripples** — gradient-noise height → slope → tilted normal. The mesh's spare `UV_0`
    channel carries the current (`flow * FLOW_TO_SPEED`), and a two-phase "flow map" drags the noise
    downstream without smearing.
  - **Soft shore** — alpha fades as thickness → 0, and on near-vertical triangles (wall faces).
- **Bug hunt: "seeds" in the water** — three causes, three fixes: (1) ripples were shading the
  *body* colour → light it in two passes (body with the smooth normal, shine/reflection with the
  rippled one); (2) fast flow stretched noise into hairs → cap drag speed + shorter cycle;
  (3) steep ripples reflected the dark ground below the horizon → ease the normal back until the
  reflection points at the sky.
- **Debug technique** — macOS blocked `screencapture`, so a throwaway system used Bevy's
  `Screenshot::primary_window()` + `save_to_disk` to grab frames for side-by-side checks.
- **Concepts** — WGSL basics, `#import`/`#ifdef` shader defs, `AsBindGroup` + `ShaderType` uniforms
  (binding 100+ to avoid StandardMaterial's slots), linear vs sRGB colour, reverse-Z depth,
  Fresnel, physically based exposure.
- **New tunables** — `WATER_ABSORPTION`, `SHORE_FADE`, `RIPPLE_STRENGTH`, `RIPPLE_SIZE`
  (replace `DEPTH_COLOR_MAX`); shallow/deep colours + `reflectance` in `setup`.

### Session 23 (Cleanup before the Bevy upgrade)

- **Decision** — Bevy 0.19 (Jun 2026) and 0.20 (Oct 8 2026) are out; we're on 0.18. No third-party
  Bevy plugins, so nothing blocks upgrading. Plan: cleanup → tests → upgrade 0.18→0.19→0.20 in
  separate steps (one migration guide at a time; tests catch sim regressions).
- **Removed dead code** — `src/camera.rs`, `src/textures.rs` (not in the module tree), the old 2D
  grid levels `levels/*.json`.
- **Trimmed `Cargo.toml`** — dropped `serde_json`, `rfd` (old save/load), `default-run` (no
  `src/bin/` left), and the direct `bevy_mesh`/`bevy_asset` deps.
- **Concept: crate re-exports** — `bevy` re-exports its sub-crates, so `bevy_mesh::Indices` is
  also `bevy::mesh::Indices`. Importing through `bevy::` keeps one version number to bump instead
  of several that must match.
- **Concept: rebasing** — main moved (PR #32) mid-session; rebased the branch onto it and resolved
  an import-line conflict in `flood.rs` by keeping both sides.
- **README** — rewritten gameplay / structure / tech-stack sections for the heightfield game.

### Session 24 (Sim unit tests — safety net for the upgrade)

- **Extracted pure helpers** from Bevy systems in `flood.rs` so tests can call them without an `App`:
  `flow_iteration(depth, floor, w, d, flow)` (one flow pass on any grid size; `step_flow` is now a
  thin wrapper), `is_floating(depth, weight)` (shared by `object_physics` *and* `build_obstacles`,
  which had diverging copies — identical for weights ≥ 60 kg), `mobility(weight)`, and
  `separate_pair(pa, wa, pb, wb)` (mass-weighted push-apart from `object_collision`).
- **13 new tests** (19 passing + 3 ignored generators): flow conserves water, never goes negative,
  levels out on flat ground, runs downhill + current points downhill; weir dams below the crest,
  spills only the overtopping layer, and crest water isn't dumped in one step; buoyancy (light
  floats, heavy grounds, dry never floats, mobility capped at 1 and falls with weight); collision
  (lighter shoved more, overlap fully removed, non-touching ignored).
- **Testing the tests (mutation check)** — temporarily reverted the session-19 weir fix: every test
  still passed, because the original dam tests never put water *on* the block. Added
  `crest_water_is_not_dumped_in_one_step`, which fails on the old rule and passes on the fix.
- **Concepts** — systems vs pure functions (Bevy `Res`/`ResMut` can't be faked cheaply; slices
  can); `&mut [f32]` accepts both a `Vec` and a fixed array; `#[cfg(test)] mod tests` +
  `use super::*` reaches private fns; small test helpers (`run_flow`, `total`); a test that never
  failed may not test anything.

### Session 25 (Bevy 0.18 → 0.19)

- **Process** — bump `bevy` in Cargo.toml, `cargo build`, group the errors by kind (15 errors → 4
  distinct changes), fix, rebuild until clean; then `cargo test` (19 pass) and `cargo run`.
- **API changes hit:**
  - `TextFont::font_size` is a `FontSize` enum → `FontSize::Px(11.0)` (also `Vw`, `Vh`, …).
  - `Atmosphere` / `ScatteringMedium` moved `bevy::pbr` → `bevy::light` (`bevy::light::atmosphere`).
  - `DirectionalLight::shadows_enabled` → `shadow_maps_enabled` (0.19 added contact shadows).
  - `Assets::get_mut` returns an `AssetMut` guard (like `Mut<T>`), so `let Some(mut mesh) = …`.
  - `Atmosphere::earthlike` → `Atmosphere::earth`, **and the atmosphere is now its own "planet"
    entity** (auto-placed one Earth radius below the origin); cameras opt in with
    `AtmosphereSettings`. This one *compiled* with just the rename — only reading the source
    showed it had to move off the camera.
- **Where to look** — the downloaded crate source in `~/.cargo/registry/src/*/bevy_*-0.19.1/` is
  the ground truth: doc comments, `pub use` re-exports, constructor names. Shader `#import`s
  (`water.wgsl`) aren't compile-checked — verified each module/function exists in 0.19.1.
- **Found, not caused by the upgrade:** the Waterfall level shows no water on the cliff face. The
  #32 shader fades near-vertical water (`smoothstep(0.3, 0.65, world_normal.y)` in `water.wgsl`)
  to hide sheets stretched up block sides — it can't tell a waterfall apart. Confirmed identical on
  main (0.18) by running it from a temp worktree. Fix after the upgrades.
- **Gotcha** — launching the built binary directly (not `cargo run`) makes Bevy look for `assets/`
  next to the executable → shader missing → invisible water. Always use `cargo run`.
- **Next blocker** — Bevy 0.20 requires Rust 1.97.1 (we have 1.95 via Homebrew).

### Session 25b (Bevy 0.19 → 0.20)

- **Toolchain** — Bevy 0.20 needs Rust ≥ 1.97.1: `brew upgrade rust` 1.95 → 1.99.1 (pulled in
  `openssl@4`, newer `ca-certificates`). Rebuilt 0.19 on the new compiler first so toolchain and
  Bevy changes weren't mixed up. (Xcode CLT is outdated for macOS 27 — blocks some brew source
  builds, didn't block rust.)
- **UI buttons reworked** — `Button` is now the headless `bevy::ui_widgets::Button`; `Interaction`
  is deprecated in favour of `bevy::ui::Pressed` (inserted while held) + picking's `Hovered`, with
  an `Activate` event on click. Handlers now query `Added<Pressed>` (same moment as the old
  `Interaction::Pressed`): marker buttons `Query<(), (Added<Pressed>, With<X>)>` + `!q.is_empty()`,
  data buttons `Query<&WeightButton, Added<Pressed>>`. (Observers on `On<Activate>` are the more
  idiomatic future style.)
- **Shaders are WESL now** — naga_oil is gone; `water.wgsl` → `water.wesl`:
  `#import a::b` → `import a::b;` (first in file), module paths follow crate file layout
  (`bevy_pbr::forward_io` → `bevy_pbr::render::forward_io`, `prepass_utils` → `prepass::utils`),
  `#ifdef X … #endif` → `@if(X)` on an import/statement/block, `#{MATERIAL_BIND_GROUP}` →
  `constants::MATERIAL_BIND_GROUP` (materials moved to bind group 3). Compiled fine in Rust —
  only a runtime check would have shown it.
- **Lesson across both upgrades** — the compiler finds renames; behaviour moves (atmosphere entity)
  and runtime-compiled assets (shaders) need reading the source/migration guide and running.

### Session 26 (Waterfalls visible)

- **Diagnosis by measurement** — a throwaway ignored test loaded the Waterfall level and ran
  `flow_iteration` to steady state, printing each row: the cliff drops 62 units in *one* 4-unit
  cell, with only ~0.2 of water on it. So the fall's surface lay on the rock (hidden, ~0
  thickness for the shore fade) *and* was faded as too steep. Shader-only tweaks couldn't fix it.
- **Tried + dropped: a separate curtain mesh** — read as a flat white wall, with a "piano key"
  staircase along diagonal lips (one strip per cell).
- **Kept: reuse the water surface** (continuous, already animated):
  - `fall_factor(normal, current)` — steep × fast × flowing downhill (the normal's xz points
    downhill). Fast water piling *up* a block's face is uphill → not a fall.
  - `uphill_cell` gate — only falls fed by wet ground; stops the sheet from a dry block top to the
    fast river behind it turning white (the shader can't see neighbours, so this is per vertex on
    the CPU and passed in `UV_1` — which also removed a Rust/WESL duplicate of the rule).
  - Fall vertices pushed `FALL_STANDOFF` *downstream* (not along the normal — normals flip along a
    stair-stepped edge and folded the sheet).
  - Shader: skip both fades on falls, white-water ribbons scrolling down, ripples off (xz noise
    smears into stripes on vertical faces), opacity × lip depth (`UV_1.y`) so thin films are wisps.
  - **Anti-aliasing with `fwidth`** — zoomed out, a pixel spans several streaks and noise aliases
    into moiré/"wood grain"; each noise octave fades to its average once `fwidth(across)` says a
    pixel covers too much of it.
- **Process** — screenshots from several angles via the throwaway Screenshot system, scripted in
  `/tmp/wf_shoot.sh` (inject → run → restore). The user's default zoomed-out view caught the
  aliasing + solid-wall problems that close-ups missed — always check the default camera too.

### Session 27 (Ground cover, scenery sets, rain-cloud storm)

- **Ground cover shader** (`terrain.wesl`, `TerrainMaterial` = `ExtendedMaterial` like the water) —
  per-pixel sand → grass by height, bare rock on slopes steeper than ~53°, large noise blotches
  (dry/lush grass, wandering sand line) + fine grain that fades with `fwidth` (no aliasing). Height
  fraction is baked into the terrain mesh's UV.x so a level switch needs no uniform update.
  Lesson: `patch` is a reserved WGSL word — shader errors only show at `cargo run`, not build.
- **Scenery sets** (`src/scenery.rs`, first split out of `flood.rs`) — trees (jittered grid + grove
  noise, pines higher up), city (grid town, streets every 3rd lot, taller toward the centre, pyramid
  roofs), ruins (broken foundations, colonnades, rubble). Level yaml `scenery: [trees, city]`
  (default `[trees]`), SCENERY panel toggles live. Fixed order city → ruins → trees sharing an
  `Occupancy` grid. Placement is pure (`plan_scenery`) and unit-tested; `hash01` makes it
  deterministic.
  - **Bevy:** change detection (`is_changed`) drives `rebuild_scenery`; `Local<T>` caches the wet
    prediction across button toggles; `children![]` + recursive despawn; shared mesh/material
    handles so hundreds of trees batch.
  - **Rust:** `let mut` for closures that mutate captures; use data before moving it into a
    resource; `.run_if()` chained twice (no `.and` in the prelude).
- **Placement measured, not guessed** — a throwaway test ran the sim and counted submerged scenery:
  the first town was 100% underwater at 5 s. Fix: `flood::predict_wet` runs the real sim for
  `DRY_SECS` (8 s) at load; nothing goes on wet cells + 1-cell margin. Needed `opt-level = 1` for
  our crate (sim ~1 s+ at opt 0 → 0.2–0.6 s). Towns now stay dry ≥15 s on all but Cascades.
- **Rain-cloud source** (`src/rain.rs`) — low-poly cloud over the source, white → storm grey with
  the rain, raindrop pool whose positions are computed from time + index (no per-drop state).
  - **Storm ramp:** inflow starts at `RAIN_START` (25%) and eases to full over `RAIN_RAMP_SECS`
    (45 s); R / level switch calm the storm. `RAIN_RATE_SCALE` = 0.75 cuts every level's rate.
  - **Water hill fixed:** the source used to pour into an 11×11 patch, piling an 80+ unit mound
    that swallowed the cloud; rain now spreads `RAIN_SPREAD` (2.5×) wider with the same total.

### Session 28 (Scenery destruction)
- **The water breaks scenery** (`src/scenery.rs`). Trees, buildings, ruin walls and standing
  columns are `Breakable`; each frame `wear_scenery` measures the water's push on them.
  - **Physics:** `water_load` = **drag** (depth × (speed / LOAD_SPEED)²) + **head** (water standing
    higher on one side, capped at its depth). Still, level water pushes evenly from all sides →
    no net force, so trees survive a calm lake. (First version counted plain depth as push —
    every wetted tree fell; measuring showed it, physics fixed it.)
  - **Wear:** above `strength`, health drains (2× strength breaks it in `BREAK_SECS` = 2 s) and
    the piece leans downstream (`tipped`: rotate about its base pivot toward the push).
  - **Collapse:** trees/columns/ruin walls topple with the current (accelerating fall); buildings
    crumble (squash + tilt) and shed 4 chunks. Fallen trees + chunks become `Debris` that floats
    on the current and sinks away after 45 s or at the map edge. R rebuilds the scenery.
  - **Damming:** buildings and ruin walls carry a `Blocker` (cells + world-height top) that
    `flood::build_obstacles` raises — water backs up behind the town. A crumbled building keeps a
    low rubble Blocker; a toppled wall loses it.
- **Bevy:** a piece's lifecycle is components — `Breakable` → `Collapsing` → `Debris` — added and
  removed with `Commands`; each system only queries its stage. `run_if` with a closure
  (`|paused: Res<Paused>| !paused.0`). **Crash learned:** commands are applied later, so a
  `rebuild_scenery` despawn in the same frame invalidated an insert → panic. Fix: `try_insert`,
  `try_remove`, `try_despawn` (silently skip despawned entities).
- **Rust:** `impl Fn(usize) -> Vec2` parameters keep `water_load` pure/testable (tests pass a
  closure, the game passes `|c| water.current(c)`); `normalize_or_zero` / `normalize_or`.
- **Tuning by measurement:** a throwaway test ran the real sim 150 s on all six levels with every
  set and counted pieces broken by 30/60/90/150 s. Final strengths: tree 6 × scale, building
  1.5 × footprint side, ruin wall 20, column 10. Full storms wreck most flooded scenery by ~90 s;
  towns on Valley/River Delta sit above the flood (need FLOOD! surges / walls to reach them).
- Tests: 50 (+6: load model, wear, tipping direction, blocker footprints).

### Session 29 (Scenery surround + shore foam)
- **Grey band fixed** (`src/surround.rs`) — a ~9 km ring of decorative land around the map:
  rings of vertices grow outward from the map's perimeter (each ring the edge square scaled up
  about the centre, spaced quadratically — dense near the map). Ring 0 *is* the map edge (exact
  heights + `terrain_normal`), so there's no seam; it eases to a smoothed edge, relaxes into
  rolling countryside (value-noise fbm), and rises into a ridged-noise mountain ring that hides
  the horizon from every allowed camera angle. Same `TerrainMaterial` as the map; rebuilt on
  level switch (`SurroundMesh` handle, like `TerrainMesh`); `NotShadowCaster`.
  - **Bevy:** `Projection::Perspective(PerspectiveProjection { far: 20_000.0, .. })` — Bevy's
    depth is infinite reverse-Z, so `far` only affects culling. The atmosphere's aerial
    perspective hazes distant land blue for free — no fog needed.
  - **Look fixes found by screenshot:** low land past the drain was a sand desert (colour comes
    from height) → UV.x eases to grass away from the edge; the 60-unit grass blotches repeated
    for km like camouflage → UV.y carries "distance out" and terrain.wesl cross-fades to a
    larger noise. Lesson: scaling noise coordinates by distance *warps* it into radial streaks;
    cross-fade two fixed-scale noises instead.
- **Shore foam** (`water.wesl`) — thin (view-ray thickness < `FOAM_WIDTH`) + fast
  (`FOAM_SPEED` range) water churns into white clumps that drift with the current (same
  two-phase flow-map trick as the ripples; fine octave fades with `fwidth`). Kept off the exact
  waterline — an opaque line there traced the water mesh's stair-stepped cell edges. First
  thresholds foamed every shore incl. the calm lake; raising `FOAM_SPEED` to 9–22 fixed it.
- Tests: 54 (+4: perimeter, ring spacing, surround height, mesh shares edge heights).

### Session 30 (FPS readout + maps twice as big)
- **FPS readout** (`src/fps.rs`, new `FpsPlugin`) — `FrameTimeDiagnosticsPlugin` records frame
  times into the `DiagnosticsStore` resource; a system reads
  `.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed())` and rewrites a top-right
  `Text` every 0.25 s (a `Local<f32>` timer — per-system state without a resource). Capped at
  ~60 by the `WinitSettings` reactive 1/60 s update mode.
- **Maps 2× bigger** — one knob, `MAP_SCALE` in `flood.rs`: grid 150² → 300² cells at the same
  `CELL`, so the world is 2× wider/deeper (heights unchanged → slopes half as steep). Levels are
  map fractions, so heightmaps/objects stretch for free; the yaml source `radius` (cells) is
  multiplied by `MAP_SCALE` so the inflow keeps pace with the 4× channel area. Camera start
  distance, max zoom and shadow cascades scale with it; the built-in valley's cell-unit
  constants too. Lesson: storing positions as fractions made the resize a few-line change.
### Session 31 (WASD camera pan)
- **WASD pans the camera** (`camera_controls`) relative to the view: W = into the screen,
  D = screen-right. Speed = `distance × 0.8 × delta_secs`, so it feels the same at any zoom
  and frame rate; diagonals are normalized. Works while paused.
- **Wave cycle moved W → Q** (W was taken by `handle_wave_buttons`).
- **Refactor for testing:** the ground-plane math moved into `pan_axes(yaw)` (shared with
  middle-drag pan), and the key reading into `wasd_axis(&ButtonInput<KeyCode>)`. Taking a plain
  reference instead of `Res<...>` lets a test build a `ButtonInput`, `.press()` keys, and check
  the result without a Bevy `App`.
- Tests: 56 (+2: pan forward matches the camera's look direction; WASD axis combines keys).

### Session 32 (Release builds missing water + ground; Linux release)

- **Bug** — the GitHub release binary showed trees, blocks and clouds but no water or ground
  (seen on CachyOS/NVIDIA). `cargo run` was fine. Cause: `release.yml` ships only the bare
  binary, but the water/terrain shaders were runtime assets in `assets/shaders/`. Bevy skips
  meshes whose material shader can't load, so only the built-in `StandardMaterial` things drew.
- **Fix — embedded assets** — shaders moved to `src/shaders/` and registered in
  `FloodPlugin::build` with `embedded_asset!(app, "shaders/water.wesl")` (imported from
  `bevy::asset::embedded_asset` — not in the prelude). `include_bytes!` bakes them into the
  binary; materials load `embedded://BlueRush/shaders/<file>`. The `assets/` folder is gone, so
  the binary runs from any directory.
- **Linux in CI** — `build.yml` and `release.yml` gained an `ubuntu-latest` job (installs
  `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`). The Linux release ships as a
  `.tar.gz` so the executable bit survives the download.
- **Concepts** — runtime assets vs embedded assets; `embedded://` asset source and how its path
  is built (crate name + path past `src/`); why a missing shader fails silently per material.

## Where We Left Off (current)

**Session 33 (`feat/breakable-walls`)** — walls break: each piece wears when water stands past ¾
of its height or pours over its top, then washes away as a light block. Next: object (block)
destruction, or polish (debris/splash on breach, damage tint).

**Session 29 (`feat/scenery-surround`)** — surround land + mountains replace the grey band;
shore foam on fast, thin water. Next: object destruction, or planar reflections.

**Session 28 (`feat/scenery-destruction`)** — scenery dams the water and is broken by it (topple,
crumble, debris that floats away). Next: object destruction, or polish (splash/dust when things
fall, cracks/tint as health drops).

**Session 27 (`feat/ground-cover`)** — ground cover shader, scenery sets (trees / city / ruins,
yaml + panel, decoration only), rain-cloud source with a building storm at 75% of the old rate.
Next: scenery destruction.

**Session 26 (`fix/waterfall-visible`)** — waterfalls render as white-water curtains; on Bevy
0.20 / Rust 1.99.1.

**Session 25 (`upgrade/bevy-0.19` → `upgrade/bevy-0.20`)** — on Bevy 0.20.0 / Rust 1.99.1. Next:
fix the waterfall fade.

**Session 24 (`test/sim-helpers`)** — sim helpers extracted + unit-tested (19 tests). Next up is
the Bevy upgrade: 0.18 → 0.19 first.

**Session 23 (`chore/cleanup`)** — dead files, old levels and unused deps removed; README current.

**Session 22 (`feat/realistic-water`)** — water now renders with a custom shader: translucent,
depth-tinted teal, flowing ripples, sky reflections from a physical atmosphere, sun shadows. Known
visual gaps: a grey band beyond the map edge (atmosphere's planet ground below the horizon); water
reflects only the sky, not banks/blocks (SSR needs opaque materials); flat-coloured terrain.


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

**Wave Off (session 5, `feat/wave-off`)** — added `WavePattern::Off` (source inflow shut off; FLOOD!
surge still works) as a fourth WAVE panel button, plus the `W` key cycling Flood → Sine → Random → Off.
Learned: adding an enum variant makes the compiler flag every non-exhaustive `match`; small
`impl` methods on enums (`next()`) keep cycle logic testable.

### Session 33 (Breakable walls)
- **Walls break** — each wall is still one `Wall` (used by erase and the drag preview), but its
  short cubes are now `WallPiece` children that dam, collide and wear on their own, so the
  water breaches the wall where it presses hardest instead of toppling the whole thing.
- **Two ways a piece gives way** (`wear_walls`, runs after `build_obstacles`):
  - **Head** — `wall_head`: how high the water stands up the wall, measured from the ground
    under the piece, beyond the water on the far side. Past `WALL_STRENGTH` (¾) of the piece's
    height it wears (`scenery::wear`).
  - **Overtopping** — `wall_overtopped`: water deeper than `WALL_WET` racing over the top faster
    than `WALL_SCOUR_SPEED` scours it as if water stood its full height, even when the far side
    has flooded too (how real levees fail).
- **Eroding** — `WallPiece::dam()` shrinks with health (down to `WALL_ERODE` of its height):
  the cube gets shorter, dams lower and gets weaker, so a failing piece keeps failing. At zero
  it's despawned and a `WALL_CHUNK` (30%) weight block is left for the current to carry off.
- **Heavier walls are taller** — `wall_height` is up to `WALL_EXTRA_HEIGHT` (30%) taller than a
  block of the same weight, so they hold back more water.
- **Tuning by playtest — lessons:**
  - Scenery's `water_load` counted drag from the current in *any* direction; water diverted
    *along* a wall wore it down, so walls felt weak however strong they were. Walls use head only.
  - `water_load` caps the head at the deepest water. On a slope the ground behind a wall is
    higher than under it, so the water's depth undercounts how high it stands up the wall —
    walls on the highland never broke. Hence `wall_head` measures from the wall's own ground.
  - Water flooding both sides leaves almost no head even while it pours over the top → the
    overtopping rule.
- **Rust** — small methods on a component (`dam()`); `pub(crate)` to share helpers across
  modules (`cells_near`, `wear`); `try_despawn` when another system may already have removed it.
- Tests: 61 (+5: erosion lowers the dam, heavy walls are taller, ¾ threshold + calm lake,
  head on a slope, overtopping needs fast water on the top).

## What Comes Next
- ~~**Breakable walls**~~ Done (session 33). Possible polish: splash/debris when a piece goes,
  darker tint as health drops, R repairs walls.
- ~~**Scenery destruction**~~ Done (session 28). Possible polish: splash/dust puffs on collapse,
  damage tint/cracks, debris colliding with blocks/walls, debris jams that dam.
- **Object destruction (next)** — reuse `water_load` + `wear` for player blocks: a strong
  enough current sweeps away or breaks them. (Walls done, session 33.)
- **Scenery realism** — ~~textured terrain~~ (27), ~~grey band → surround land~~ and
  ~~shore foam~~ (29) done. Remaining (optional): planar reflections of banks/blocks (second
  camera render); sparse trees on the near surround; snow/rock on the far mountains.
- **Walls (optional)** — curved/polyline walls. (Breaking done, session 33.)
- **Crest-edge polish (optional)** — the upstream overtopping sheet sticks out ~1 cell past the
  block back; needs finer geometry near obstacles if it ever bothers us.
- ~~**Config + cleanup**~~ Done (session 23).
- ~~**Tests**~~ Done (session 24). Not yet covered: `build_obstacles` footprint mask, wall
  collision — candidates if the upgrade touches them.
- ~~**Waterfall fade bug**~~ Done (session 26). Possible polish: splash/mist at the base of falls.
- ~~**Bevy upgrade**~~ Done (session 25): 0.18 → 0.19 → 0.20. Bump to 0.20.1 when released.
- **Buttons → observers (optional, learning)** — rewrite the panel handlers as `On<Activate>`
  observers, the 0.20-native pattern.
  Known 0.20 change: pointer events flattened (`Pointer<Press>` → `PointerPress`).
- ~~**Levels** — author terrain heightmaps; save/load.~~ Done (session 20): PNG heightmap +
  yaml levels, see `docs/LEVELS.md`. Remaining: more shipped levels, in-game level picker.
