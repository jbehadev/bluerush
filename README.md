# BlueRush

A flood simulation game built with Rust and the [Bevy](https://bevyengine.org/) game engine. Water flows over a heightfield terrain, pools, and carries objects in its path. Place blocks of different weights and watch them get swept away, pile up, or dam the stream.

## Gameplay

- **Place blocks** of varying weight — light ones get carried by the current, heavy ones dam and divert it
- **Draw walls** by click-and-drag to channel the flow
- **Pour water** anywhere, or trigger a FLOOD! surge
- **Pick a wave pattern** for the source (Flood, Sine, Random, Off)
- **Load levels** from `levels/*.yaml` + PNG heightmaps (see [docs/LEVELS.md](docs/LEVELS.md))

For full controls and mechanics, see the **[Manual](MANUAL.md)**.

## Screenshots

> Orbit-camera view of a stream bed with depth-shaded water and a UI panel

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
  main.rs   — App setup, window, plugin registration
  config.rs — AppConfig loaded from config.yaml (window size, level path)
  flood.rs  — FloodPlugin: terrain, water sim, objects, walls, UI, camera, input
  level.rs  — Level yaml + PNG heightmap loading
levels/     — Shipped levels (*.yaml + *.png heightmaps)
assets/     — shaders/water.wesl (custom water material)
docs/       — WATER_FLOW.md (how the sim works), LEVELS.md (level format)
```

## Tech Stack

| Crate | Purpose |
|-------|---------|
| [bevy 0.18](https://bevyengine.org/) | Game engine (ECS, rendering, input) |
| [rand 0.8](https://crates.io/crates/rand) | Random wave pattern |
| [serde](https://serde.rs/) + serde_yaml | Config and level files |
| [image 0.25](https://crates.io/crates/image) | PNG heightmap decode/encode |

## License

MIT
