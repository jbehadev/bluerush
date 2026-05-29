use rand::Rng;
use crate::simulation::{Cell, MAX_WATER_KG, water_fill, flow_capacity};

/// A 3D voxel simulation grid.
///
/// Axis convention:
///   X — left/right
///   Y — vertical; gravity pulls toward Y=0 (ground floor)
///   Z — front/back (depth)
///
/// Index formula: y * width * depth + z * width + x
pub struct Grid3D {
    pub width:  usize,
    pub height: usize,
    pub depth:  usize,
    pub cells:  Vec<Cell>,
}

impl Grid3D {
    /// Row-major 3D flat index.
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        y * self.width * self.depth + z * self.width + x
    }

    /// Create an all-Air grid with no pre-placed geometry.
    pub fn blank(width: usize, height: usize, depth: usize) -> Self {
        Grid3D { width, height, depth, cells: vec![Cell::Air; width * height * depth] }
    }

    pub fn set_cell(&mut self, x: usize, y: usize, z: usize, cell: Cell) {
        let i = self.idx(x, y, z);
        self.cells[i] = cell;
    }

    pub fn get_cell(&self, x: usize, y: usize, z: usize) -> &Cell {
        &self.cells[self.idx(x, y, z)]
    }

    /// Returns true if (x, y, z) is inside the grid bounds.
    pub fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0
            && (x as usize) < self.width
            && (y as usize) < self.height
            && (z as usize) < self.depth
    }
}

// ---------------------------------------------------------------------------
// Water flow simulation
// ---------------------------------------------------------------------------

/// Advance the 3D water simulation one tick using a gravity-biased delta-buffer
/// diffusion algorithm.
///
/// Neighbor weights:
///   Y-down (gravity): 0.50  — dominant downward pull
///   X±, Z± (lateral): 0.10  — lateral spreading
///   Y-up (pressure):  0.05  — only when pressure below forces water up
///
/// All transfers accumulate in a delta buffer before being applied so that
/// water mass is conserved (no creation from multiple simultaneous writes).
pub fn step_simulation_3d(grid: &Grid3D) -> Vec<Cell> {
    let n = grid.cells.len();
    let mut delta = vec![0.0f32; n];

    const NEIGHBORS: [(i32, i32, i32, f32); 6] = [
        (0, -1, 0, 0.50),  // down  — gravity priority
        (-1, 0, 0, 0.10),  // left
        (1,  0, 0, 0.10),  // right
        (0,  0,-1, 0.10),  // front
        (0,  0, 1, 0.10),  // back
        (0,  1, 0, 0.05),  // up   — pressure only
    ];

    for y in 0..grid.height {
        for z in 0..grid.depth {
            for x in 0..grid.width {
                let idx = grid.idx(x, y, z);
                let fill = match water_fill(&grid.cells[idx]) {
                    Some(f) => f,
                    None => continue,
                };

                for &(dx, dy, dz, factor) in &NEIGHBORS {
                    let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                    if !grid.in_bounds(nx, ny, nz) { continue; }
                    let nidx = grid.idx(nx as usize, ny as usize, nz as usize);
                    if let Some(nfill) = flow_capacity(&grid.cells[nidx]) {
                        let transfer = (fill - nfill).max(0.0) * factor;
                        delta[idx]  -= transfer;
                        delta[nidx] += transfer;
                    }
                }
            }
        }
    }

    let mut new_cells = grid.cells.clone();
    for i in 0..n {
        if delta[i] == 0.0 { continue; }
        let current = flow_capacity(&grid.cells[i]).unwrap_or(0.0);
        let new_kg = (current + delta[i]).clamp(0.0, MAX_WATER_KG);
        new_cells[i] = if new_kg < 1.0 { Cell::Air } else { Cell::Water(new_kg) };
    }

    // Preserve permanent fixture cells
    for i in 0..n {
        match grid.cells[i] {
            Cell::Spring          => new_cells[i] = Cell::Spring,
            Cell::Drain           => new_cells[i] = Cell::Drain,
            Cell::Rock            => new_cells[i] = Cell::Rock,
            Cell::Sand            => new_cells[i] = Cell::Sand,
            Cell::Wall            => new_cells[i] = Cell::Wall,
            Cell::Building { .. } => new_cells[i] = grid.cells[i].clone(),
            _ => {}
        }
    }

    new_cells
}

// ---------------------------------------------------------------------------
// Depth pressure
// ---------------------------------------------------------------------------

/// Compute per-cell depth-based pressure for a 3D grid.
///
/// Scans each (x, z) column from the top (y = height-1) downward to y=0,
/// accumulating water mass with an exponential decay. Cells deeper in the
/// column receive higher pressure values.
pub fn build_depth_pressure_3d(grid: &Grid3D) -> Vec<f32> {
    let mut pressure = vec![0.0f32; grid.cells.len()];
    let decay: f32 = 0.1f32.powf(1.0 / 10.0);
    let inlet_pressure: f32 = MAX_WATER_KG * 3.0;

    for z in 0..grid.depth {
        for x in 0..grid.width {
            // Scan BOTTOM (y=0) to TOP (y=height-1), accumulating pressure from
            // water below. The implicit inlet seed at y=0 ensures every column
            // has some base pressure so objects near the ground still feel force.
            // Pressure increases with y (more accumulated water below = higher
            // upward push), which is what drives buoyancy in the +y direction.
            let mut water_below: Vec<(f32, usize)> = vec![(inlet_pressure, 0)];

            for y in 0..grid.height {
                let i = grid.idx(x, y, z);
                let p: f32 = water_below
                    .iter()
                    .map(|&(kg, wy)| kg * decay.powi(y as i32 - wy as i32))
                    .sum();

                match &grid.cells[i] {
                    Cell::Water(kg) => {
                        pressure[i] = p;
                        water_below.push((*kg, y));
                    }
                    Cell::Object(w) => {
                        pressure[i] = (p - w).max(0.0);
                    }
                    Cell::Spring => {
                        pressure[i] = p;
                        water_below.push((MAX_WATER_KG, y));
                    }
                    Cell::Wall | Cell::Rock => {
                        pressure[i] = 0.0;
                        water_below.clear();
                    }
                    Cell::Building { .. } => {
                        pressure[i] = p;
                        water_below.clear();
                    }
                    Cell::Air | Cell::Drain | Cell::Sand => {
                        water_below.clear();
                        pressure[i] = 0.0;
                    }
                }
            }
        }
    }
    pressure
}

// ---------------------------------------------------------------------------
// Object physics
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct MoveIntent3D {
    src:    usize,
    dst:    usize,
    weight: f32,
}

/// Advance 3D object physics one tick using the 3-pass MoveIntent system.
///
/// Objects are pushed upward by buoyancy (depth pressure) and laterally by
/// horizontal pressure differences. An anti-oscillation deadzone prevents
/// jitter on the X and Z axes when pressure is nearly balanced.
pub fn step_objects_3d(grid: &mut Grid3D, rng: &mut impl Rng, collision_destruction: bool) {
    let pressure = build_depth_pressure_3d(grid);
    let threshold = 0.1f32;

    // Pass 1: collect intended moves
    let mut intents: Vec<MoveIntent3D> = Vec::new();

    for y in 0..grid.height {
        for z in 0..grid.depth {
            for x in 0..grid.width {
                let idx = grid.idx(x, y, z);
                let weight = match &grid.cells[idx] {
                    Cell::Object(w) => *w,
                    _ => continue,
                };

                let p = pressure[idx];
                if p <= 0.0 { continue; }

                let net_y = (p - weight).max(0.0);

                let pl = if x > 0 { pressure[grid.idx(x-1, y, z)] } else { 0.0 };
                let pr = if x+1 < grid.width  { pressure[grid.idx(x+1, y, z)] } else { 0.0 };
                let pf = if z > 0 { pressure[grid.idx(x, y, z-1)] } else { 0.0 };
                let pb = if z+1 < grid.depth  { pressure[grid.idx(x, y, z+1)] } else { 0.0 };

                let x_force = pl - pr;
                let z_force = pf - pb;
                let net_x = (x_force.abs() - weight).max(0.0);
                let net_z = (z_force.abs() - weight).max(0.0);

                let x_stable = x_force.abs() < (pl + pr) * 0.05;
                let z_stable = z_force.abs() < (pf + pb) * 0.05;

                let (dx, dy, dz): (i32, i32, i32) =
                    if net_y >= net_x.max(net_z) && net_y > threshold {
                        (0, 1, 0)
                    } else if net_x >= net_z && net_x > threshold && !x_stable {
                        (x_force.signum() as i32, 0, 0)
                    } else if net_z > threshold && !z_stable {
                        (0, 0, z_force.signum() as i32)
                    } else {
                        (0, 0, 0)
                    };

                if dx == 0 && dy == 0 && dz == 0 { continue; }

                let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                if !grid.in_bounds(nx, ny, nz) { continue; }

                intents.push(MoveIntent3D {
                    src: idx,
                    dst: grid.idx(nx as usize, ny as usize, nz as usize),
                    weight,
                });
            }
        }
    }

    // Pass 2: one winner per destination (random tie-break)
    let mut by_dst: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for (i, intent) in intents.iter().enumerate() {
        by_dst.entry(intent.dst).or_default().push(i);
    }
    let mut winners: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for candidates in by_dst.values() {
        winners.insert(candidates[rng.r#gen::<usize>() % candidates.len()]);
    }

    // Pass 3: apply winning moves
    let mut moved: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut new_cells = grid.cells.clone();

    let mut sorted: Vec<usize> = winners.into_iter().collect();
    sorted.sort_by(|&a, &b| intents[b].src.cmp(&intents[a].src));

    for &i in &sorted {
        let intent = &intents[i];
        let blocked = matches!(
            new_cells[intent.dst],
            Cell::Wall | Cell::Spring | Cell::Drain | Cell::Rock | Cell::Building { .. }
        ) || (matches!(new_cells[intent.dst], Cell::Object(_)) && !moved.contains(&intent.dst));

        if !blocked {
            let vacated = new_cells[intent.dst].clone();
            new_cells[intent.dst] = Cell::Object(intent.weight);
            new_cells[intent.src] = vacated;
            moved.insert(intent.src);
        } else if collision_destruction
            && matches!(new_cells[intent.dst], Cell::Object(_))
            && !moved.contains(&intent.dst)
        {
            let victim_destroyed = if let Cell::Object(ref mut vw) = new_cells[intent.dst] {
                *vw -= intent.weight;
                *vw <= 0.0
            } else { false };
            if victim_destroyed { new_cells[intent.dst] = Cell::Air; }
            new_cells[intent.src] = Cell::Air;
        }
    }
    grid.cells = new_cells;
}

// ---------------------------------------------------------------------------
// Column-fill tools
// ---------------------------------------------------------------------------

/// Compute the cells a column-fill click at column (x, z) should change. The
/// fill "drops" from the active layer straight down to the floor: every cell
/// from Y=0 up to `active_layer_y` (inclusive) for which `can_fill` returns
/// true is included. Cells above the active layer are left untouched.
///
/// The caller supplies `can_fill` (which existing cells may be replaced) and
/// decides what to fill them with — so the same routine builds a solid Wall
/// dam or a stack of Objects. Returns `(x, y, z, old_cell)` for each changed
/// cell so the caller can record undo history before applying.
pub fn column_changes(
    grid: &Grid3D,
    x: usize,
    z: usize,
    active_layer_y: usize,
    can_fill: impl Fn(&Cell) -> bool,
) -> Vec<(usize, usize, usize, Cell)> {
    let mut changes = Vec::new();
    if x >= grid.width || z >= grid.depth {
        return changes;
    }
    // Clamp the top of the fill to the active layer (and to the grid ceiling).
    let top = active_layer_y.min(grid.height.saturating_sub(1));
    for y in 0..=top {
        let old = grid.get_cell(x, y, z);
        if can_fill(old) {
            changes.push((x, y, z, old.clone()));
        }
    }
    changes
}

/// Wall column-fill: fills every cell from the active layer down to the floor
/// that is not permanent terrain (Rock / Sand) or an existing Wall, building
/// a water-tight dam.
pub fn wall_column_changes(
    grid: &Grid3D,
    x: usize,
    z: usize,
    active_layer_y: usize,
) -> Vec<(usize, usize, usize, Cell)> {
    column_changes(grid, x, z, active_layer_y, |c| {
        !matches!(c, Cell::Rock | Cell::Sand | Cell::Wall)
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Task 1 — Grid3D struct
    #[test]
    fn blank_grid_is_all_air() {
        let g = Grid3D::blank(4, 3, 5);
        assert_eq!(g.width, 4);
        assert_eq!(g.height, 3);
        assert_eq!(g.depth, 5);
        assert!(g.cells.iter().all(|c| matches!(c, Cell::Air)));
    }

    #[test]
    fn idx_round_trips() {
        let g = Grid3D::blank(10, 8, 12);
        let (x, y, z) = (3, 5, 7);
        let i = g.idx(x, y, z);
        assert_eq!(i, y * g.width * g.depth + z * g.width + x);
    }

    // Wall column-fill tool
    #[test]
    fn wall_column_drops_from_layer_to_floor() {
        let mut g = Grid3D::blank(5, 20, 5);
        // Rock floor at Y=0
        for z in 0..5 { for x in 0..5 { g.set_cell(x, 0, z, Cell::Rock); } }

        // Click the Wall tool at active layer 15.
        let changes = wall_column_changes(&g, 2, 2, 15);

        // Fills Y=1..=15 (15 cells); the Rock floor at Y=0 is skipped.
        assert_eq!(changes.len(), 15);
        assert!(changes.iter().all(|&(x, y, z, _)| x == 2 && z == 2 && (1..=15).contains(&y)));

        for &(x, y, z, _) in &changes { g.set_cell(x, y, z, Cell::Wall); }
        assert!(matches!(g.get_cell(2, 0, 2), Cell::Rock), "floor stays Rock");
        for y in 1..=15 { assert!(matches!(g.get_cell(2, y, 2), Cell::Wall), "y={y} should be Wall"); }
        for y in 16..20 { assert!(matches!(g.get_cell(2, y, 2), Cell::Air), "above the layer stays Air"); }
    }

    #[test]
    fn wall_column_skips_existing_solid_cells() {
        let mut g = Grid3D::blank(3, 10, 3);
        g.set_cell(1, 0, 1, Cell::Rock);   // floor
        g.set_cell(1, 2, 1, Cell::Wall);   // an existing wall partway up
        let changes = wall_column_changes(&g, 1, 1, 4);
        // Y=0 Rock and Y=2 Wall are skipped → only Y=1, 3, 4 change.
        let ys: Vec<usize> = changes.iter().map(|&(_, y, _, _)| y).collect();
        assert_eq!(ys, vec![1, 3, 4]);
    }

    #[test]
    fn wall_column_clamps_layer_to_grid_ceiling() {
        let g = Grid3D::blank(3, 5, 3);
        // An active layer beyond the grid height clamps to the top (Y=4).
        let changes = wall_column_changes(&g, 1, 1, 99);
        assert_eq!(changes.len(), 5); // Y=0..=4, all Air
    }

    #[test]
    fn wall_column_out_of_bounds_is_empty() {
        let g = Grid3D::blank(3, 5, 3);
        assert!(wall_column_changes(&g, 5, 1, 2).is_empty());
        assert!(wall_column_changes(&g, 1, 9, 2).is_empty());
    }

    // Object column-fill: stacks blocks down the empty column, leaving
    // terrain and fixtures (Rock / Spring / etc.) intact.
    #[test]
    fn object_column_fills_only_air_cells() {
        let mut g = Grid3D::blank(3, 10, 3);
        g.set_cell(1, 0, 1, Cell::Rock);    // floor
        g.set_cell(1, 3, 1, Cell::Spring);  // a fixture partway up the column
        let changes = column_changes(&g, 1, 1, 5, |c| matches!(c, Cell::Air));
        // Only the Air cells (Y=1, 2, 4, 5) fill; the Rock floor and the
        // Spring are left untouched.
        let ys: Vec<usize> = changes.iter().map(|&(_, y, _, _)| y).collect();
        assert_eq!(ys, vec![1, 2, 4, 5]);
    }

    #[test]
    fn object_column_stacks_from_floor_to_layer() {
        let mut g = Grid3D::blank(4, 20, 4);
        for z in 0..4 { for x in 0..4 { g.set_cell(x, 0, z, Cell::Rock); } }
        let changes = column_changes(&g, 2, 2, 12, |c| matches!(c, Cell::Air));
        // Y=1..=12 are Air → 12 cells fill; the Rock floor (Y=0) is skipped.
        assert_eq!(changes.len(), 12);
        assert!(changes.iter().all(|&(_, y, _, _)| (1..=12).contains(&y)));
    }

    // Task 2 — step_simulation_3d
    #[test]
    fn water_falls_down_not_up() {
        let mut g = Grid3D::blank(1, 3, 1);
        g.set_cell(0, 2, 0, Cell::Water(MAX_WATER_KG));
        let next = step_simulation_3d(&g);
        assert!(matches!(next[g.idx(0, 1, 0)], Cell::Water(f) if f > 0.0),
            "Water should fall to y=1");
        assert!(!matches!(next[g.idx(0, 0, 0)], Cell::Water(_)),
            "Water should not skip two layers in one tick");
    }

    #[test]
    fn water_is_conserved_3d() {
        let mut g = Grid3D::blank(3, 3, 3);
        for y in 0..3 { for z in 0..3 {
            g.set_cell(0, y, z, Cell::Wall); g.set_cell(2, y, z, Cell::Wall);
        }}
        for y in 0..3 { for x in 0..3 {
            g.set_cell(x, y, 0, Cell::Wall); g.set_cell(x, y, 2, Cell::Wall);
        }}
        for x in 0..3 { for z in 0..3 {
            g.set_cell(x, 0, z, Cell::Wall); g.set_cell(x, 2, z, Cell::Wall);
        }}
        g.set_cell(1, 1, 1, Cell::Water(MAX_WATER_KG));

        let before: f32 = g.cells.iter()
            .filter_map(|c| if let Cell::Water(f) = c { Some(*f) } else { None })
            .sum();

        let mut grid = g;
        for _ in 0..10 { grid.cells = step_simulation_3d(&grid); }

        let after: f32 = grid.cells.iter()
            .filter_map(|c| if let Cell::Water(f) = c { Some(*f) } else { None })
            .sum();

        assert!((before - after).abs() < 0.5,
            "Water should be conserved; lost {:.2}", before - after);
    }

    // Task 3 — depth pressure + objects
    #[test]
    fn depth_pressure_increases_upward() {
        // Pressure accumulates from the bottom upward — higher y = more water
        // below = more accumulated upward push.
        let mut g = Grid3D::blank(1, 4, 1);
        for y in 0..4 { g.set_cell(0, y, 0, Cell::Water(MAX_WATER_KG)); }
        let p = build_depth_pressure_3d(&g);
        assert!(p[g.idx(0, 1, 0)] >= p[g.idx(0, 0, 0)],
            "y=1 should have >= pressure of y=0");
        assert!(p[g.idx(0, 2, 0)] >= p[g.idx(0, 1, 0)],
            "y=2 should have >= pressure of y=1");
    }

    #[test]
    fn light_object_rises_in_water() {
        let mut g = Grid3D::blank(1, 4, 1);
        g.set_cell(0, 0, 0, Cell::Water(MAX_WATER_KG));
        g.set_cell(0, 1, 0, Cell::Water(MAX_WATER_KG));
        g.set_cell(0, 2, 0, Cell::Object(10.0));
        let mut rng = rand::thread_rng();
        step_objects_3d(&mut g, &mut rng, false);
        assert!(matches!(g.get_cell(0, 3, 0), Cell::Object(_)),
            "Light object should rise to y=3");
    }

    #[test]
    fn heavy_object_stays_3d() {
        let mut g = Grid3D::blank(1, 4, 1);
        g.set_cell(0, 0, 0, Cell::Water(MAX_WATER_KG));
        g.set_cell(0, 1, 0, Cell::Water(MAX_WATER_KG));
        g.set_cell(0, 2, 0, Cell::Object(5000.0));
        let mut rng = rand::thread_rng();
        step_objects_3d(&mut g, &mut rng, false);
        assert!(matches!(g.get_cell(0, 2, 0), Cell::Object(_)),
            "Heavy object should not move");
    }
}
