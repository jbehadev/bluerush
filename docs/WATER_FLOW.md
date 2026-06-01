# BlueRush — How the Water Flows

This explains the heightfield water simulation introduced on the
`feat/heightfield-water` branch. It all lives in **`src/flood.rs`**, wired into
the app by **`FloodPlugin`** (`main.rs` just opens the window and adds the plugin,
so `cargo run` launches the game).

---

## 1. Mental model

The world is a grid of **columns**, `W × D` of them, `CELL` world-units apart.
Two height fields are stored per column:

- **`Terrain(Vec<f32>)`** — the immovable ground: a meandering, downhill
  *stream bed* carved into rising banks (`terrain_height`, `channel_center`).
- **`Water { depth, ripple, rvel, flow }`** — `depth[i]` is how much water sits
  on column `i`. The **water surface** elevation is `terrain[i] + depth[i]`.

Indexing is row-major: `idx(x, z) = z * W + x`. World position of a column comes
from `cell_to_world` / the `-half()` offset that centres the grid on the origin.

Water never teleports: each tick it only moves to neighbours whose **surface** is
lower, and never more than it actually holds. That one rule makes it behave like
real water — it runs downhill, pools to a flat level, and spills over obstacles.

---

## 2. The per-frame pipeline

`FloodPlugin` runs the simulation systems in a fixed order each frame
(`.chain()`):

| # | System | Does |
|---|--------|------|
| 1 | `drain_on_key`     | `R` empties all water |
| 2 | `run_source`       | adds water at the top of the stream (with the wave pattern) |
| 3 | `handle_click`     | applies the selected tool (place block / pour / erase) |
| 4 | `build_obstacles`  | grounded blocks raise an effective floor (damming) |
| 5 | `step_flow`        | **the core water movement** |
| 6 | `step_ripples`     | a visual-only ripple layer for liveliness |
| 7 | `object_physics`   | buoyancy + carrying objects on the current |
| 8 | `object_collision` | stop objects overlapping |
| 9 | `sync_objects`     | copy object positions onto their cubes |
| 10| `update_water_mesh`| rebuild the water surface mesh |

A separate (order-independent) group handles UI buttons, the orbit camera, the
placement cursor, and pause.

When **paused** (`Space`), the simulation systems (2, 5–8) early-return; input,
camera, and rendering keep running, so you can still build and look around.

---

## 3. The source — `run_source`

Water enters at the top of the channel every frame. The **wave pattern** (a
`Wave` resource) modulates how much:

| Pattern | Behaviour |
|---------|-----------|
| **Flood**  | steady inflow |
| **Sine**   | smoothly pulsing — `0.5 + 0.5·sin(t · SINE_FREQ)` |
| **Random** | gusty — a new random multiplier every `RANDOM_INTERVAL` seconds |

`add_water` deposits depth into a small patch and nudges the ripple field so the
inflow looks alive. Base rate is `SOURCE_RATE`.

---

## 4. The flow algorithm — `step_flow`

The heart of it: a mass-conserving **"water finds its level"** relaxation, run
`FLOW_ITERS` times per frame for fast settling. For each water column `i`:

1. Compute its **surface**: `si = floor(i) + depth[i]`, where
   `floor = terrain + obstacle` (obstacle = damming from grounded blocks, §6).
2. Look at the 4 neighbours; collect those with a **lower** surface and their
   `gap = si − sj`.
3. It wants to move about **half** of each gap (`gap · 0.5 · FLOW_RATE`), but the
   total outflow is **scaled down so it never sends more water than it has**.
4. Each transfer goes into a delta buffer (`delta[i] -= out; delta[j] += out`) and
   is recorded as a **current** vector (`flow[i] += direction · out`).
5. After every column is processed, all deltas are applied at once — so the order
   of iteration can't create or destroy water (conservation).

Finally the **front edge drains** (water reaching the bottom of the channel runs
off), which keeps a sustained current instead of pooling to a standstill.

**Why it pools flat:** moving half the surface gap toward equal, capped by
available water, converges to neighbours sharing one surface level — a flat pond —
and then stops (no gap → no flow).

---

## 5. The current, and carrying objects — `object_physics`

`step_flow` leaves a per-column **`flow: Vec2`** — the net direction and amount
water moved this frame (the *current*). Objects (`FloatObject`) use it:

- An object **floats** when the water is deep enough for its weight
  (`depth · BUOYANCY ≥ weight`); otherwise it rests on the terrain
  (height eased by `VERT_EASE`, sitting `DRAFT` below the surface when afloat).
- While floating it **drifts toward the local current** (`flow · FLOW_TO_SPEED`),
  scaled by `mobility = REF_WEIGHT / weight` and eased by `FLOW_EASE`. So light
  blocks ride the flow and heavy blocks barely move — "heavy resists, light gets
  carried."

`object_collision` then separates overlapping objects with a **mass-weighted**
push (the lighter one moves more), so a heavy block holds its ground and others
pile against it.

---

## 6. Damming — `build_obstacles` + the obstacle floor

A **grounded** block (too heavy to float at its current depth) raises the
effective floor (`Obstacle`) under its footprint by the block's height.
`step_flow` uses `terrain + obstacle` as the floor, so water **backs up behind**
the block and **diverts around** it. Block height and footprint scale with weight
(`obj_height`, `obj_footprint`), so heavier blocks dam more and seal wider. Once
the water fills to the block's top it **overtops** and spills over.

> Floating blocks don't obstruct (they ride on top), which keeps it physical.
>
> *Known limitation:* overtopping currently shows as the water level rising
> against the dam and spilling around/over — not a literal cresting sheet on top
> of the blocks. A clean "weir-model" version is planned (see `PROGRESS.md`).

---

## 7. Rendering the surface — `update_water_mesh`

A single deforming mesh (`W × D` vertices) is rebuilt each frame:

- Each vertex height = `terrain[i] + depth[i] + ripple` (ripple fades out in
  shallow water via `RIPPLE_FADE`).
- Per-vertex **normals** are computed from neighbour heights so the surface
  catches light (the "watery" sheen).
- Only **wet** quads are emitted — any corner with `depth > WET` — giving a clean
  shoreline; dry columns sit at terrain height so the water tapers to the bank.
- A translucent, low-roughness `StandardMaterial` (white base, so per-vertex
  colours drive the look) lets the floor show through.
- **Depth shading** — per-vertex colour lerps **shallow → deep** (light, clear
  blue → dark, more opaque, reaching darkest at `DEPTH_COLOR_MAX`), so pooled and
  dammed water reads as deep while the flowing sheet reads as shallow.

`step_ripples` drives the `ripple` field with a small damped wave equation
(`RIPPLE_SPEED`, `RIPPLE_DAMP`, capped at `MAX_RIPPLE`) purely for visual
liveliness — it does not affect the actual water volume.

The **terrain** mesh (`build_terrain_mesh`) is height-shaded the same way:
per-vertex colour lerps **light brown (low streambed) → green (high banks)** over
`0 .. SLOPE_HEIGHT + BANK_MAX`, so elevation reads at a glance.

---

## 8. Controls

| Input | Action |
|-------|--------|
| Left-click | apply the selected tool (place block of the chosen weight / pour water / erase) |
| Right-drag | orbit camera (`camera_controls`) |
| Middle-drag | pan camera |
| Scroll | zoom |
| `Space` | pause / resume the simulation |
| `R` | drain all water |

Left panel: **OBJECTS** weight buttons (200–5000 kg), **Pour Water**, **Erase**,
and **WAVE** patterns (Flood / Sine / Random). The cursor shows a placement
preview — a wireframe box sized to the selected weight, or a marker for
pour/erase (`draw_placement_cursor`).

---

## 9. New methods introduced on this branch (quick reference)

**Terrain / geometry**
- `terrain_height(x, z)` — the stream-bed height field (slope + meander + banks)
- `channel_center(z)` — the meandering centre line of the stream
- `cell_to_world`, `cell_of`, `idx`, `half`, `span` — grid ↔ world helpers
- `build_terrain_mesh` — static terrain mesh

**Water simulation**
- `run_source` — wave-pattern-modulated inflow
- `add_water` — deposit depth + kick ripples in a patch
- `step_flow` — the relaxation flow + current field + edge drain
- `step_ripples` — visual ripple wave equation
- `build_obstacles` — grounded-block damming floor
- `update_water_mesh` — rebuild the clipped, lit water surface

**Objects**
- `obj_footprint`, `obj_height`, `weight_t`, `weight_color` — weight → size/colour
- `spawn_object` — create a weighted block
- `object_physics` — buoyancy + current-carry
- `object_collision` — mass-weighted separation

**Input / UI / camera**
- `handle_click` — left-click tool dispatch (place / pour / erase)
- `setup_ui`, `handle_weight_buttons`, `handle_pour_button`, `handle_erase_button`,
  `handle_wave_buttons`, `update_tool_highlight`, `update_wave_highlight`
- `camera_controls` — orbit / pan / zoom (`OrbitCamera`)
- `draw_placement_cursor` — placement preview gizmo
- `toggle_pause`, `handle_pause_button`, `update_pause_button` — pause (`Paused`)
- `drain_on_key` — `R` to drain

---

## 10. Key tunables (top of `flood.rs`)

`W`, `D`, `CELL` (grid) · `SLOPE_HEIGHT`, `MEANDER_AMPL`, `MEANDER_FREQ`,
`CHANNEL_HW`, `BANK_K`, `BANK_MAX` (terrain) · `SOURCE_RATE`, `SINE_FREQ`,
`RANDOM_INTERVAL` (source) · `FLOW_RATE`, `FLOW_ITERS`, `WET` (flow) ·
`RIPPLE_SPEED`, `RIPPLE_DAMP`, `MAX_RIPPLE`, `RIPPLE_FADE` (ripples) ·
`DEPTH_COLOR_MAX` (water depth shading) ·
`BUOYANCY`, `DRAFT`, `VERT_EASE`, `FLOW_TO_SPEED`, `FLOW_EASE`, `REF_WEIGHT`
(objects) · `OBJ_FOOTPRINT_MIN/MAX`, `OBJ_HEIGHT_MIN/MAX` (weight→size) ·
`PANEL_WIDTH`, `WEIGHTS` (UI).
