# BlueRush

A 2D/3D flood simulation game built with Rust and the [Bevy](https://bevyengine.org/) game engine. Water rushes in, spreads via pressure diffusion, and carries objects in its path. Place blocks of different weights and watch physics play out.

## Gameplay

- **Place objects** of varying weight (200–5000 kg) on the grid
- **Activate water** to flood the grid from the bottom
- **Watch** as lighter objects float and get carried; heavier ones act as barriers
- **Place springs** to create localized water sources
- **Save and load** your layouts to experiment freely

For full controls and mechanics, see the **[Manual](MANUAL.md)**.

## Screenshots

> 3D isometric view with pressure heatmap, hover cursor, and UI toolbar

## Building & Running

Requires [Rust](https://rustup.rs/) (stable, 2024 edition).

```bash
# Clone the repo
git clone https://github.com/jbehadev/BlueRush.git
cd BlueRush

# Build and run
cargo run
```

> **Note:** The first build will take a while as Bevy and its dependencies compile. Subsequent builds are fast — dependencies are compiled with `opt-level = 3` while your code stays at `opt-level = 0` for quick iteration.

## Running Tests

```bash
cargo test
```

## Project Structure

```
src/
  main.rs        — App setup, plugin registration
  grid.rs        — GridPlugin, all UI/rendering/input systems, 3D camera, gizmos
  simulation.rs  — Cell/Grid types, step_simulation, step_objects, build_depth_pressure
  textures.rs    — TexturesPlugin, programmatic froth textures
  persistence.rs — Save/load grid to JSON files
```

## Tech Stack

| Crate | Purpose |
|-------|---------|
| [bevy 0.18](https://bevyengine.org/) | Game engine (ECS, rendering, input) |
| [rand 0.8](https://crates.io/crates/rand) | Random conflict resolution in object physics |
| [serde](https://serde.rs/) + serde_json | Grid serialization for save/load |
| [rfd 0.15](https://crates.io/crates/rfd) | Native file dialogs (save/open) |

## License

MIT
