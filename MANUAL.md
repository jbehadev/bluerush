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
| R | Reset grid (clear all cells) |
| 1 | Select 200 kg block |
| 2 | Select 500 kg block |
| 3 | Select 1000 kg block |
| Cmd+S | Save grid to file |
| Cmd+O | Load grid from file |

### Toolbar (Left Panel)
- **Weight buttons** — select block weight (200 / 500 / 1000 kg)
- **Eraser** — switch to eraser tool to remove placed cells
- **Spring** — place a water spring source
- **Inlet toggle** — open/close the water inlet gate
- **Heatmap** — toggle pressure heatmap visualization
- **Speed +/-** — adjust simulation speed (1x to 16x)
- **Brush +/-** — adjust brush size (1x1, 3x3, 5x5, ...)

## Visual Feedback
- **Hover cursor** — yellow wireframe rectangles show which cells your brush will affect before clicking
- **Water gradient** — deeper blue = more water in the cell
- **Tile height** — 3D tiles grow taller based on content (water fill, walls, objects)
- **Heatmap mode** — rainbow overlay showing pressure distribution (blue = low, red = high)

## Game Mechanics
- **Water** flows in from the bottom row when activated and spreads via pressure diffusion
- **Light objects** (200 kg) get pushed around easily by water pressure
- **Heavy objects** (1000 kg) resist water flow and can act as barriers
- **Springs** continuously generate water at their location
- **Walls** are immovable border tiles that cannot be overwritten
- **Objects float** — buoyancy pushes objects upward based on depth pressure
