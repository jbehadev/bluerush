# BlueRush Manual

## Overview
BlueRush is a flood simulation game where water rushes in and carries objects in its path. Place objects of different weights on a grid, activate the water, and watch physics play out.

## Controls

### Mouse
| Action | Effect |
|--------|--------|
| Left click / hold | Place selected tool on grid |
| Right click + Shift | Debug inspect cell (prints to console) |
| Middle click + drag | Pan the camera |
| Scroll wheel | Zoom in / out |

### Keyboard
| Key | Effect |
|-----|--------|
| X | Toggle water flow on/off |
| W | Cycle inlet mode (Flood → Sine → Random) |
| R | Reset grid (clear all cells) |
| 1 | Select 200 kg block |
| 2 | Select 500 kg block |
| 3 | Select 1000 kg block |
| 4 | Select 2000 kg block |
| 5 | Select 5000 kg block |
| E | Select eraser tool |
| S | Select spring tool |
| D | Select drain tool |
| B | Select building tool |
| M | Toggle pressure heatmap |
| F | Toggle flow arrows |
| Shift + drag | Constrain placement to a straight line |
| Cmd+Z | Undo |
| Cmd+Shift+Z | Redo |
| Cmd+S | Save grid to file |
| Cmd+O | Load grid from file |

### Toolbar (Left Panel)

**Objects**
- **Weight buttons** — select block weight (200 / 500 / 1000 / 2000 / 5000 kg)
- **Eraser** — remove placed cells
- **Spring** — place a water spring source
- **Drain** — place a water drain sink
- **Building** — place a destructible building (collapses under water pressure)

**Brush**
- **+/-** — adjust brush size (1x1, 3x3, 5x5, ...)

**Flow**
- **Let it flow / Stop the flow** — toggle the water inlet gate on/off
- **Flood / Sine / Random** — select inlet mode:
  - **Flood** — constant maximum water flow
  - **Sine** — smooth oscillating wave (~4 second cycle, 100–1000 kg)
  - **Random** — sine wave with a random peak each cycle

**View**
- **Normal** — standard cell colours
- **Pressure** — rainbow heatmap of depth-based pressure
- **Flow** — arrows showing water flow direction

**Other**
- **Reset** — clear all cells and stop water
- **Speed +/-** — adjust simulation speed (1x to 16x)

## Visual Feedback
- **Hover cursor** — yellow wireframe rectangles show which cells your brush will affect before clicking
- **Water gradient** — deeper blue = more water in the cell
- **Tile height** — 3D tiles grow taller based on content (water fill, walls, objects)
- **Heatmap mode** — rainbow overlay showing pressure distribution (blue = low, red = high)

## Game Mechanics
- **Water** flows in from the inlet row when activated and spreads via pressure diffusion
- **Light objects** (200 kg) get pushed around easily by water pressure
- **Heavy objects** (5000 kg) resist water flow and can act as barriers
- **Springs** continuously generate water at their location
- **Drains** continuously remove water at their location
- **Buildings** are destructible structures that collapse into debris when water pressure exceeds their threshold
- **Walls** are immovable border tiles that cannot be overwritten
- **Objects float** — buoyancy pushes objects upward based on depth pressure
- **Collision destruction** — fast-moving objects can destroy lighter objects on impact
