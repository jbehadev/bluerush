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

## What Was Built
- Blue box rendered on screen with `Sprite::from_color`
- Camera with `Camera2d`
- Box positioned with `Transform::from_xyz`
- Box moving diagonally using delta time in an `Update` system
- Refactored into `GridPlugin` in `src/grid.rs`
- `Cell` enum defined for grid cell types

## Current File Structure
```
src/
  main.rs   — app setup, registers GridPlugin
  grid.rs   — GridPlugin, Cell enum, MyBox marker, setup & move_system
```

## Where We Left Off
About to design the `Grid` struct — a Bevy Resource holding a flat `Vec<Cell>` with width/height.

## What Comes Next
- Define `Grid` struct with `width`, `height`, `Vec<Cell>`
- Register it as a Bevy Resource
- Render the grid as tiles on screen
