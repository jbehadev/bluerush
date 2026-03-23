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

## What Comes Next
- **Harbour Inlet level** — author the level layout in `levels/harbour-inlet.json`
- **Level select UI** — button/menu to switch between loaded level files
- **Water rendering polish** — animated water surface, transparency, or wave effects
- **Object interaction** — dragging placed objects, object-to-object collision
- **Sound effects** — water flow, object placement, splash sounds
- **Performance** — profile with large grids, consider chunk-based updates
