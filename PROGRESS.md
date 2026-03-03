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

## Where We Left Off
UI panel with weight selection is working. Grid shrunk by PANEL_HEIGHT to make room.

`hold_the_line` test still failing (pre-existing issue from Session 6).

## What Comes Next
- **Fix `build_depth_pressure`** — include the cell's own fill in its depth value, or rethink the pressure model so water directly below an object contributes to upward force
- **`hold_the_line` test should pass** after the fix
- **`pressure` field on `MoveIntent`** — still unused, dead code warning pending
