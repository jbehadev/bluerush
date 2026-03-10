# TODO — BlueRush Game Improvements

## Visual Polish
- [ ] Add ambient light so cube faces not hit by the directional light aren't pure black
- [ ] Vary object tile height by weight (heavier = taller) so they're visually distinguishable beyond color shade
- [ ] Add weight labels or icons on object tiles
- [ ] Water transparency or animated surface effect using StandardMaterial properties
- [ ] Splash particle effects when objects land in water

## New Gameplay Mechanics
- [ ] Conveyor tiles that push objects in a fixed direction
- [ ] Wind/fan force sources that apply directional pressure without water
- [ ] Allow objects to interact before inlet is opened (step_objects currently gated behind water_flow)

## Quality of Life / UX
- [ ] In-game toast notifications for save/load success/errors (currently console-only)
- [ ] Grid coordinate overlay or mini-map
- [ ] Fix brush size label to show "NxN" instead of just "N"
- [ ] Add missing keyboard shortcuts to MANUAL.md (E for eraser, S for spring, M for heatmap, Home for camera reset)
- [ ] Show visual feedback when file dialog is already open

## Level / Challenge System
- [ ] Predefined puzzle levels with win conditions (e.g. "get the block to the exit zone")
- [ ] Level editor with save/load
- [ ] Star ratings based on time or number of blocks used
- [ ] Level select screen

## Architecture / Code Health
- [ ] Interleave step_objects and step_simulation at high sim speeds instead of batching separately
- [ ] Remove or integrate unused TextureAssets from textures.rs
