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

## Android Port
- [ ] **Touch input** — all interaction reads `ButtonInput<MouseButton>`/`KeyCode`
      (`src/grid.rs:162`, `src/camera.rs:91`); Android emits neither. Needs
      `Touches`/`TouchInput` handling.
- [ ] **Camera gestures** — replace middle-drag pan and scroll zoom with
      two-finger drag and pinch (`src/camera.rs`)
- [ ] **Gesture disambiguation** — single-finger paint vs. drag-to-pan conflict
- [ ] **Touchless shortcuts** — undo/redo, Shift line-constraint, debug inspect
      and Home camera-reset have no toolbar equivalent
- [ ] **Responsive toolbar** — fixed-pixel left panel (`src/ui.rs`) is oversized
      on a phone, especially in portrait
- [ ] **Profile rendering on device** — 1,554 per-tile mesh entities with
      per-frame material/mesh mutation (`src/render.rs:283`, `:324`, `:374`)
- [ ] **Save/load on Android** — `rfd` has no Android backend; currently stubbed
      out in `src/persistence.rs`. Needs SAF or app-private storage.
- [ ] Add armeabi-v7a/x86_64 ABIs to the APK if wider device support is wanted

## Architecture / Code Health
- [ ] Interleave step_objects and step_simulation at high sim speeds instead of batching separately
- [ ] Remove or integrate unused TextureAssets from textures.rs
