# Levels — edit the terrain in an image editor, not in code

A level is a pair of files in `levels/`:

| File | What it is |
|------|------------|
| `<name>.yaml` | Level metadata: which heightmap to use, how tall it is, where the water source sits, and any starting objects. |
| `<name>.png` | A **grayscale heightmap**. Black = lowest ground, white = highest. This *is* the terrain — the game lifts it into 3D at load. |

`config.yaml` picks which level the game plays:

```yaml
level: levels/valley.yaml
```

If that file doesn't exist, the game writes an editable template there on
first run (generated from the built-in valley terrain), so you always have a
working starting point to modify.

## Editing the terrain

Open the level's `.png` in any image editor (Preview, GIMP, Photoshop,
Aseprite, …) and paint:

- **Darker = lower.** Paint a dark line and water will find and follow it.
- **Lighter = higher.** Paint bright borders to wall water in.
- The image can be **any size** — it is smoothly resampled onto the game's
  150×150 simulation grid at load. A 150×150 image is 1 pixel per cell;
  bigger images just give you more comfortable brushwork.
- 8-bit or 16-bit grayscale both work (the shipped templates are 16-bit for
  smooth slopes; 8-bit gives 256 height steps, which is usually fine).
- Color images also load — they're converted to luminance — but authoring in
  grayscale is much easier to reason about.

Things to keep in mind when sculpting:

- **The front edge of the map (bottom row of the image) is the drain** —
  water that reaches it runs off. Give your terrain a downhill path toward it
  if you want a flowing stream; wall it off with a bright row if you want the
  map to fill up like a basin.
- The image's top row is the **back** of the map (where the camera initially
  looks toward); left/right in the image are left/right in the world.
- Soft gradients (use a blur or soft brush) make natural slopes; hard edges
  make cliffs.

## The yaml

```yaml
name: Valley
heightmap: valley.png   # PNG path, relative to this yaml file
height_scale: 160.0     # world height of a pure-white pixel (black = 0)
source:                 # where water enters the map
  x: 0.59               # 0.0 = left edge … 1.0 = right edge
  z: 0.03               # 0.0 = back edge … 1.0 = front edge (the drain)
  radius: 5             # source patch radius in cells (wider = gentler)
  rate: 110.0           # water depth added per second
objects:                # starting blocks (optional)
  - { x: 0.71, z: 0.50, weight: 4000.0 }   # heavy — dams the stream
  - { x: 0.31, z: 0.34, weight: 150.0 }    # light — washes away
```

All positions are **fractions of the map (0.0–1.0)**, so a level is
independent of both the image size and the simulation grid resolution.
Weights use the same scale as the in-game buttons (200–5000 kg); an object's
size, height, and damming power all follow from its weight.

## Shipped levels

Switch in-game with the **LEVEL** dropdown at the top of the toolbar (it
lists every `.yaml` in the levels directory), or set the startup level with
`level:` in `config.yaml`. Switching resets the water and restores the
level's starting objects:

| Level | File | What happens |
|-------|------|--------------|
| Valley | `levels/valley.yaml` | The original meandering stream bed (the default). A light / medium / heavy block trio sits in the channel. |
| Winding River | `levels/winding-river.yaml` | Four tight S-bends with steep banks — the current whips around the corners. A 3000 kg block grounds at a bend and forces the river over its banks. |
| River Delta | `levels/river-delta.yaml` | A steep upper valley splits into three distributaries fanning across a flat marshy mouth. A 2500 kg block sits right on the split — erase or move it to redirect which branches run. |
| Highland Lake | `levels/highland-lake.yaml` | The source fills a deep basin behind a ridge; the only exit is a narrow spill notch. Watch the lake rise to the sill and pour down the runout — a 2000 kg block starts as a plug in the notch. |

## Making a new level

1. Copy `levels/valley.yaml` + `levels/valley.png` to a new name, or just
   point `config.yaml` at a path that doesn't exist yet and run the game once
   to get a fresh template.
2. Paint the PNG, tweak the yaml.
3. Set `level:` in `config.yaml` to your new yaml and run. Errors (missing or
   unreadable files) are printed to the console and the game falls back to
   the built-in valley.

## Regenerating the shipped levels

The committed level files are baked from generator functions in
`src/flood.rs` (`terrain_height` for the valley; the `generate_extra_levels`
test for the rest). To re-bake after changing them:

```sh
cargo test generate_valley_level -- --ignored
cargo test generate_extra_levels -- --ignored
```

Hand-edits to the PNGs survive until you re-run these — the generators are
seeds, the files on disk are the source of truth at runtime.
