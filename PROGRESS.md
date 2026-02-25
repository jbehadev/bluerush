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

## What Was Built
- `Cell::Water(f32)` fill level with color gradient rendering
- Border walls on left, right, and top edges using `Object(9999.0)`
- `flow_water` system — fills bottom row each tick when enabled
- `simulate_flow` system — pressure-based diffusion using delta buffer; water spreads in all four directions equally
- `GameState` resource — `X` key toggles water flow, `R` key resets grid
- `step_simulation` pure function — simulation logic extracted from Bevy system for testability
- Two unit tests: spread detection and water conservation check

## Current File Structure
```
src/
  main.rs   — app setup, registers GridPlugin
  grid.rs   — GridPlugin, Cell/Grid/Tile/GameState types, all systems and simulation logic
```

## Where We Left Off
Water simulation working — fills from bottom inlet, spreads with pressure gradient showing as color (deep blue = full, light blue = partial). Objects placed by mouse act as barriers. Delta buffer ensures water is conserved across ticks.

## What Comes Next
- Gravity bias — make downward transfer stronger so water pools at bottom naturally
- Froth rendering — low fill levels render with a foamy/white color
- Object interaction — light objects pushed by water pressure, heavy ones immovable
