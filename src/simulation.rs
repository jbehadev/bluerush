use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const MAX_WATER_KG: f32 = 1000.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cell {
    Air,
    Water(f32),
    Object(f32),
    Wall,
    Spring, // fixed water source; always holds MAX_WATER_KG
    Drain,  // fixed water sink; always holds 0 kg
}

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn init(width: usize, height: usize) -> Grid {
        let mut grid = Grid {
            width,
            height,
            cells: vec![Cell::Air; width * height],
        };

        for x in 0..width {
            grid.set_cell(x, height - 1, Cell::Wall);
        }
        for y in 0..height {
            grid.set_cell(0, y, Cell::Wall);
            grid.set_cell(width - 1, y, Cell::Wall);
        }

        // Gate row: y=1 interior cells start as Wall, opened by animate_gate
        if height > 1 {
            for x in 1..width - 1 {
                grid.set_cell(x, 1, Cell::Wall);
            }
        }

        // Pre-fill y=0 as the reservoir — the gate at y=1 controls flow into the simulation.
        for x in 1..width - 1 {
            grid.set_cell(x, 0, Cell::Water(MAX_WATER_KG));
        }

        grid
    }

    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        self.cells[y * self.width + x] = cell;
    }

    pub fn get_cell(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.width + x]
    }
}

fn water_fill(cell: &Cell) -> Option<f32> {
    match cell {
        Cell::Water(f) => Some(*f),
        Cell::Spring => Some(MAX_WATER_KG),
        _ => None,
    }
}

fn flow_capacity(cell: &Cell) -> Option<f32> {
    match cell {
        Cell::Water(f) => Some(*f),
        Cell::Air => Some(0.0),
        Cell::Drain => Some(0.0), // drain appears empty — water always flows in
        _ => None,
    }
}

pub fn step_simulation(grid: &Grid) -> Vec<Cell> {
    let width = grid.width;
    let height = grid.height;

    // delta[i] accumulates the net change in water for each cell this tick.
    // Positive = gaining water, negative = losing water.
    let mut delta = vec![0.0f32; width * height];

    for y in 0..height {
        for x in 1..width - 1 {
            let idx = y * width + x;

            let fill = match water_fill(&grid.cells[idx]) {
                Some(f) => f,
                None => continue,
            };

            // For each passable neighbor, calculate transfer and record it
            // as a loss for this cell and a gain for the neighbor.
            let neighbors: &[(isize, isize)] = &[(0, -1), (0, 1), (-1, 0), (1, 0)];
            for (dx, dy) in neighbors {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                    continue;
                }
                let nidx = ny as usize * width + nx as usize;
                if let Some(neighbor_fill) = flow_capacity(&grid.cells[nidx]) {
                    let transfer = (fill - neighbor_fill).max(0.0) * 0.25;
                    delta[idx] -= transfer; // this cell loses water
                    delta[nidx] += transfer; // neighbor gains water
                }
            }
        }
    }

    // Apply all deltas at once to produce the new cell state
    let mut new_cells = grid.cells.clone();
    for i in 0..new_cells.len() {
        if delta[i] == 0.0 {
            continue;
        }
        let current = flow_capacity(&grid.cells[i]).unwrap_or(0.0);
        let new_kg = (current + delta[i]).clamp(0.0, MAX_WATER_KG);
        new_cells[i] = if new_kg < 1.0 {
            Cell::Air
        } else {
            Cell::Water(new_kg)
        };
    }

    // Preserve spring and drain cells — they are permanent fixtures
    for i in 0..new_cells.len() {
        match grid.cells[i] {
            Cell::Spring => new_cells[i] = Cell::Spring,
            Cell::Drain => new_cells[i] = Cell::Drain,
            _ => {}
        }
    }

    new_cells
}

fn water_pressure(cell: &Cell) -> f32 {
    match cell {
        Cell::Water(f) => *f,
        _ => 0.0,
    }
}

fn object_weight(cell: &Cell) -> Option<f32> {
    match cell {
        Cell::Object(w) => Some(*w),
        _ => None,
    }
}

// Represents an object's intention to move this tick.
#[derive(Debug)]
struct MoveIntent {
    src: usize,                // index of the object's current cell
    dst: usize,                // index of the cell it wants to move into
    weight: f32,               // the object's weight
    pressure: f32,             // pressure from the pushing side
    fallback_dst: Option<usize>, // secondary direction to try if primary is blocked
}

pub fn build_depth_pressure(grid: &Grid) -> Vec<f32> {
    let width = grid.width;
    let height = grid.height;
    let mut depth = vec![0.0f32; width * height];
    let decay: f32 = 0.1f32.powf(1.0 / 10.0);
    let inlet_pressure: f32 = MAX_WATER_KG * 3.0;

    for x in 0..width {
        let mut water_below: Vec<(f32, usize)> = vec![(inlet_pressure, 0)]; // inlet as seed
        for y in 0..height {
            let pressure: f32 = water_below
                .iter()
                .map(|&(kg, wy)| kg * decay.powi(y as i32 - wy as i32))
                .sum();

            match grid.cells[y * width + x] {
                Cell::Water(kg) => {
                    depth[y * width + x] = pressure;
                    water_below.push((kg, y));
                }
                Cell::Object(weight) => {
                    depth[y * width + x] = (pressure - weight).max(0.0);
                    // don't push
                }
                Cell::Spring => {
                    depth[y * width + x] = pressure;
                    water_below.push((MAX_WATER_KG, y));
                }
                Cell::Wall => {
                    depth[y * width + x] = 0.0;
                    water_below.clear(); // walls block pressure from above
                }
                Cell::Air | Cell::Drain => {
                    water_below.clear();
                    depth[y * width + x] = 0.0;
                }
            }
        }
    }
    depth
}

/// BFS from inlet (y=0) through connected non-wall cells.
/// Returns distance-from-inlet for each cell (u32::MAX = unreachable).
/// Objects should move toward HIGHER distance values (downstream).
pub fn build_flow_distance(grid: &Grid) -> Vec<u32> {
    let width = grid.width;
    let height = grid.height;
    let mut dist = vec![u32::MAX; width * height];
    let mut queue = VecDeque::new();

    // Seed from inlet row (y=0)
    for x in 0..width {
        if matches!(grid.cells[x], Cell::Water(_) | Cell::Spring) {
            dist[x] = 0;
            queue.push_back(x);
        }
    }

    while let Some(idx) = queue.pop_front() {
        let d = dist[idx];
        let x = idx % width;
        let y = idx / width;

        for (ddx, ddy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;
            if dist[nidx] <= d + 1 {
                continue;
            }
            match grid.cells[nidx] {
                Cell::Wall => {} // walls block flow path
                _ => {
                    dist[nidx] = d + 1;
                    queue.push_back(nidx);
                }
            }
        }
    }

    dist
}

pub fn step_objects(grid: &Grid) -> Vec<Cell> {
    let width = grid.width;
    let height = grid.height;

    // Build depth-based pressure table so cells deeper in the water column
    // feel higher pressure regardless of local fill level equalisation.
    let depth = build_depth_pressure(grid);

    // Flow distance: BFS distance from inlet. Objects move toward higher
    // distance (downstream) instead of always pushing in +y.
    let flow_dist = build_flow_distance(grid);

    // Pass 1: collect all intended moves without writing anything yet.
    let mut intents: Vec<MoveIntent> = Vec::new();
    for y in (0..height).rev() {
        for x in 0..width {
            let idx = y * width + x;

            let weight = match object_weight(&grid.cells[idx]) {
                Some(w) => w,
                None => continue,
            };

            // Horizontal pressure: use depth[] from adjacent water cells,
            // consistent with the vertical buoyancy calculation.
            let p_left = if x > 0 {
                match &grid.cells[y * width + (x - 1)] {
                    Cell::Water(_) => depth[y * width + (x - 1)],
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let p_right = if x < width - 1 {
                match &grid.cells[y * width + (x + 1)] {
                    Cell::Water(_) => depth[y * width + (x + 1)],
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let x_force = p_left - p_right;

            // Vertical pressure: depth[idx] for objects is (raw_upward_pressure - weight).max(0.0).
            // A positive value means buoyancy exceeds the object's weight — no need
            // to compare against weight again.
            let y_force = depth[idx];

            // Both net forces are now comparable: y_force already has weight subtracted
            // (via depth[]), so subtract weight from x_force the same way.
            let net_x = (x_force.abs() - weight).max(0.0);
            let net_y = y_force;

            let threshold = 0.1;

            // Anti-oscillation: require the horizontal pressure difference to be a
            // meaningful fraction of the average pressure.  Without this, objects
            // jitter left-right when pressure is nearly equal on both sides.
            let avg_pressure = (p_left + p_right) * 0.5;
            let horizontal_deadzone = avg_pressure * 0.1; // need >10% imbalance to move
            let x_stable = x_force.abs() < horizontal_deadzone;

            // Use flow distance to determine downstream direction.
            // This follows the river path around bends instead of always
            // pushing in +y (which shoves objects into walls at turns).
            let obj_fd = flow_dist[idx];
            let mut downstream_dx = 0.0f32;
            let mut downstream_dy = 0.0f32;
            if obj_fd != u32::MAX {
                for (ddx, ddy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                    let fnx = x as isize + ddx;
                    let fny = y as isize + ddy;
                    if fnx < 0 || fny < 0 || fnx >= width as isize || fny >= height as isize {
                        continue;
                    }
                    let fidx = fny as usize * width + fnx as usize;
                    let nd = flow_dist[fidx];
                    if nd != u32::MAX && nd > obj_fd {
                        downstream_dx += ddx as f32;
                        downstream_dy += ddy as f32;
                    }
                }
            }
            let has_flow = downstream_dx != 0.0 || downstream_dy != 0.0;

            let (dx, dy) = if net_y >= net_x && net_y > threshold {
                // Buoyancy is the dominant force — use flow direction if available
                if has_flow {
                    if downstream_dx.abs() >= downstream_dy.abs() {
                        (downstream_dx.signum() as isize, 0isize)
                    } else {
                        (0isize, downstream_dy.signum() as isize)
                    }
                } else {
                    (0isize, 1isize) // fallback: original +y behavior
                }
            } else if net_x > threshold && !x_stable {
                (x_force.signum() as isize, 0isize)
            } else if has_flow && (net_y > threshold || net_x > threshold) {
                // Neither axis dominates but there's some force — follow flow
                if downstream_dx.abs() >= downstream_dy.abs() {
                    (downstream_dx.signum() as isize, 0isize)
                } else {
                    (0isize, downstream_dy.signum() as isize)
                }
            } else {
                (0isize, 0isize)
            };

            if dx == 0 && dy == 0 {
                continue;
            }

            let pushing_pressure = net_x.max(net_y);

            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;

            // Fallback direction: if the primary move is blocked, try the secondary axis.
            // - Primary UP → try horizontal (x_force direction, or column parity if symmetric)
            // - Primary LEFT/RIGHT → try UP if there is buoyancy
            let fallback_dst = if dy != 0 {
                // Primary is vertical; fall back to horizontal.
                let fb_dx: isize = if x_force > threshold {
                    1
                } else if x_force < -threshold {
                    -1
                } else {
                    // No clear horizontal preference — alternate by column so a wide
                    // block doesn't all pile to one side.
                    if x % 2 == 0 { -1 } else { 1 }
                };
                let fb_nx = x as isize + fb_dx;
                if fb_nx >= 0 && fb_nx < width as isize {
                    Some(y * width + fb_nx as usize)
                } else {
                    None
                }
            } else {
                // Primary is horizontal; fall back to up if buoyancy is present.
                if net_y > threshold && y + 1 < height {
                    Some((y + 1) * width + x)
                } else {
                    None
                }
            };

            intents.push(MoveIntent {
                src: idx,
                dst: nidx,
                weight,
                pressure: pushing_pressure,
                fallback_dst,
            });
        }
    }

    // Pass 2: group intents by destination.
    // For each destination, if multiple objects want it, pick one randomly.
    let mut by_dst: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, intent) in intents.iter().enumerate() {
        by_dst.entry(intent.dst).or_default().push(i);
    }

    // Build a set of winning intent indices — one per destination, chosen randomly.
    let mut rng = rand::thread_rng();
    let mut winners: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for candidates in by_dst.values() {
        let winner = candidates[rng.r#gen::<usize>() % candidates.len()];
        winners.insert(winner);
    }

    // Pass 3: apply winning moves, skipping any whose src is another winner's dst.
    // This prevents a winner from overwriting a cell that another winner is vacating into.
    let mut moved_srcs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Need to sort by y axis with top-down order
    let mut sorted_winners: Vec<usize> = winners.into_iter().collect();
    sorted_winners.sort_by(|&a, &b| intents[b].src.cmp(&intents[a].src));
    let mut new_cells = grid.cells.clone();
    for &i in &sorted_winners {
        let intent = &intents[i];

        // Helper: returns true if the cell at `idx` blocks entry.
        let is_blocked = |idx: usize| -> bool {
            matches!(new_cells[idx], Cell::Wall | Cell::Spring | Cell::Drain)
                || (matches!(new_cells[idx], Cell::Object(_)) && !moved_srcs.contains(&idx))
        };

        // Try primary direction first; fall back to secondary if blocked.
        let effective_dst = if !is_blocked(intent.dst) {
            Some(intent.dst)
        } else if let Some(fb) = intent.fallback_dst {
            if !is_blocked(fb) { Some(fb) } else { None }
        } else {
            None
        };

        if let Some(dst) = effective_dst {
            let vacated = new_cells[dst].clone();
            new_cells[dst] = Cell::Object(intent.weight);
            new_cells[intent.src] = vacated;
            moved_srcs.insert(intent.src);
        }
    }

    new_cells
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a small grid manually for testing
    fn make_grid(width: usize, height: usize, cells: Vec<Cell>) -> Grid {
        Grid {
            width,
            height,
            cells,
        }
    }

    #[test]
    fn object_pushed_by_water() {
        // 3x3 grid:
        //   y=2: [Air,     Air,      Air]
        //   y=1: [Air,     Object,   Air]   ← object at (1,1), weight 10.0
        //   y=0: [Air,     Water,    Air]   ← water at (1,0), fill 1.0 = 1000kg
        //
        // Water below pushes upward. Force = 1.0 * 1000 = 1000kg > 10kg weight.
        // Object should move from (1,1) to (1,2). Source (1,1) should become water.
        let mut cells = vec![Cell::Air; 9];
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // (1,0)
        cells[1 * 3 + 1] = Cell::Object(10.0); // (1,1)
        let grid = make_grid(3, 3, cells);

        let result = step_objects(&grid);

        // Object should have moved to (1,2)
        assert!(
            matches!(result[2 * 3 + 1], Cell::Object(_)),
            "Object should have moved to (1,2)"
        );
        // Source (1,1) should now be water
        assert!(
            matches!(result[1 * 3 + 1], Cell::Air),
            "Vacated cell (1,1) should be air"
        );
    }

    #[test]
    fn heavy_object_stays_put() {
        // Same layout but object weighs 2000kg — more than 1000kg of water pressure.
        let mut cells = vec![Cell::Air; 15];
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG);
        cells[1 * 3 + 1] = Cell::Water(MAX_WATER_KG);
        cells[2 * 3 + 1] = Cell::Object(4000.0);
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        // Object should NOT have moved
        assert!(
            matches!(result[2 * 3 + 1], Cell::Object(_)),
            "Heavy object should stay at (2,1)"
        );
        // Cell above should still be air
        assert!(
            matches!(result[3 * 3 + 1], Cell::Air),
            "Cell above should remain air"
        );
    }

    #[test]
    fn water_spreads_to_air_neighbor() {
        // 3x3 grid, water at center (x=1, y=1), everything else Air
        let mut cells = vec![Cell::Air; 9];
        cells[1 * 3 + 1] = Cell::Water(MAX_WATER_KG);
        let grid = make_grid(3, 3, cells);

        let result = step_simulation(&grid);

        let above_idx = 2 * 3 + 1;
        let has_water_above = matches!(result[above_idx], Cell::Water(f) if f > 0.0);
        assert!(has_water_above, "Water should have spread upward to (1,2)");
    }

    #[test]
    fn water_is_conserved_over_ticks() {
        // 5x5 grid with walls on edges, water in the middle
        // Walls at x=0, x=4, y=0, y=4 — water at (2,2)
        let mut cells = vec![Cell::Air; 25];
        for x in 0..5 {
            cells[0 * 5 + x] = Cell::Wall;
        }
        for x in 0..5 {
            cells[4 * 5 + x] = Cell::Wall;
        }
        for y in 0..5 {
            cells[y * 5 + 0] = Cell::Wall;
        }
        for y in 0..5 {
            cells[y * 5 + 4] = Cell::Wall;
        }
        cells[2 * 5 + 2] = Cell::Water(MAX_WATER_KG);

        let mut grid = make_grid(5, 5, cells);

        // Count total water before
        let total_before: f32 = grid
            .cells
            .iter()
            .filter_map(|c| {
                if let Cell::Water(f) = c {
                    Some(*f)
                } else {
                    None
                }
            })
            .sum();

        // Run 10 ticks
        for _ in 0..10 {
            grid.cells = step_simulation(&grid);
        }

        // Count total water after
        let total_after: f32 = grid
            .cells
            .iter()
            .filter_map(|c| {
                if let Cell::Water(f) = c {
                    Some(*f)
                } else {
                    None
                }
            })
            .sum();

        println!("Water before: {total_before:.4}, after: {total_after:.4}");
        assert!(
            (total_before - total_after).abs() < 0.1,
            "Water should be conserved, lost {:.4}",
            total_before - total_after
        );
    }

    #[test]
    fn vertical_line_no_merge() {
        let mut cells = vec![Cell::Air; 5 * 3];
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=0
        cells[1 * 3 + 1] = Cell::Object(200.0); // y=1
        cells[2 * 3 + 1] = Cell::Object(200.0); // y=2
        cells[3 * 3 + 1] = Cell::Object(200.0); // y=3
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        let object_count = result
            .iter()
            .filter(|c| matches!(c, Cell::Object(_)))
            .count();
        assert_eq!(object_count, 3, "Should still have exactly 3 objects");
    }

    #[test]
    fn sandwiched_objects_float() {
        // Simulates the user's reported scenario:
        // A column of 2 objects with water BOTH below and above them.
        //
        //   y=4: [Air, Air, Air]
        //   y=3: [Air, Water, Air]  ← water ABOVE
        //   y=2: [Air, Object, Air] ← top object
        //   y=1: [Air, Object, Air] ← bottom object
        //   y=0: [Air, Water, Air]  ← water BELOW
        let mut cells = vec![Cell::Air; 5 * 3];
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=0 water below
        cells[1 * 3 + 1] = Cell::Object(200.0);        // y=1 bottom object
        cells[2 * 3 + 1] = Cell::Object(200.0);        // y=2 top object
        cells[3 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=3 water above
        let grid = make_grid(3, 5, cells);

        let depth = super::build_depth_pressure(&grid);
        println!("depth y=0 (water): {:.2}", depth[0 * 3 + 1]);
        println!("depth y=1 (object): {:.2}", depth[1 * 3 + 1]);
        println!("depth y=2 (object): {:.2}", depth[2 * 3 + 1]);
        println!("depth y=3 (water): {:.2}", depth[3 * 3 + 1]);

        let result = step_objects(&grid);
        println!("result y=0: {:?}", result[0 * 3 + 1]);
        println!("result y=1: {:?}", result[1 * 3 + 1]);
        println!("result y=2: {:?}", result[2 * 3 + 1]);
        println!("result y=3: {:?}", result[3 * 3 + 1]);
        println!("result y=4: {:?}", result[4 * 3 + 1]);

        // Both objects should have moved up by 1
        assert!(
            matches!(result[2 * 3 + 1], Cell::Object(_)),
            "Bottom object should be at y=2"
        );
        assert!(
            matches!(result[3 * 3 + 1], Cell::Object(_)),
            "Top object should be at y=3"
        );
        assert!(
            !matches!(result[1 * 3 + 1], Cell::Object(_)),
            "y=1 should no longer have an object"
        );
    }

    #[test]
    fn blocked_upward_slips_sideways() {
        // A column of objects pressed against the ceiling.
        // Primary direction (UP) is blocked by the Wall at y=4.
        // The fallback should slip the top object sideways, freeing the chain.
        //
        //   y=4: [Wall, Wall, Wall]  ← ceiling
        //   y=3: [Air,  Obj,  Air]   ← top object — UP blocked, fallback RIGHT (x=1 is odd)
        //   y=2: [Air,  Obj,  Air]
        //   y=1: [Air,  Obj,  Air]
        //   y=0: [Air,  Water, Air]  ← water provides upward pressure
        //
        // With NO fallback (old behaviour): nothing can move — top is stuck against ceiling,
        // each object below is blocked by the unmoved one above.
        //
        // With fallback: (1,3) slips RIGHT to (2,3); (1,2) moves up to (1,3); (1,1) to (1,2).
        let mut cells = vec![Cell::Air; 5 * 3];
        // Ceiling
        cells[4 * 3 + 0] = Cell::Wall;
        cells[4 * 3 + 1] = Cell::Wall;
        cells[4 * 3 + 2] = Cell::Wall;
        // Column of objects
        cells[3 * 3 + 1] = Cell::Object(200.0); // (1,3)
        cells[2 * 3 + 1] = Cell::Object(200.0); // (1,2)
        cells[1 * 3 + 1] = Cell::Object(200.0); // (1,1)
        // Water below
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // (1,0)
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        assert_eq!(
            result.iter().filter(|c| matches!(c, Cell::Object(_))).count(),
            3,
            "Object count must be preserved"
        );
        // Top object must have slipped right to (2,3)
        assert!(
            matches!(result[3 * 3 + 2], Cell::Object(_)),
            "Top object should have slipped sideways to (2,3)"
        );
        // Chain below should have shifted up: (1,2) and (1,3) now hold the lower two objects
        assert!(
            matches!(result[3 * 3 + 1], Cell::Object(_)),
            "Object from y=2 should now be at (1,3)"
        );
        assert!(
            matches!(result[2 * 3 + 1], Cell::Object(_)),
            "Object from y=1 should now be at (1,2)"
        );
        // Bottom slot should be vacated
        assert!(
            !matches!(result[1 * 3 + 1], Cell::Object(_)),
            "(1,1) should be vacated after chain shifts up"
        );
    }

    #[test]
    fn hold_the_line() {
        let mut cells = vec![Cell::Air; 5 * 3];
        cells[0 * 3 + 0] = Cell::Water(MAX_WATER_KG); // y=0
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=0
        cells[0 * 3 + 2] = Cell::Water(MAX_WATER_KG); // y=0
        cells[1 * 3 + 0] = Cell::Water(MAX_WATER_KG); // y=1
        cells[1 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=1
        cells[1 * 3 + 2] = Cell::Water(MAX_WATER_KG); // y=1
        cells[2 * 3 + 0] = Cell::Object(200.0); // y=2
        cells[2 * 3 + 1] = Cell::Object(200.0); // y=2
        cells[2 * 3 + 2] = Cell::Object(200.0); // y=2
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        let object_count = result
            .iter()
            .filter(|c| matches!(c, Cell::Object(_)))
            .count();
        assert_eq!(object_count, 3, "Should still have exactly 3 objects");
        assert!(
            !matches!(result[2 * 3 + 0], Cell::Object(_)),
            "not object at (0,2)"
        );
        assert!(
            !matches!(result[2 * 3 + 1], Cell::Object(_)),
            "not object at (1,2)"
        );
        assert!(
            !matches!(result[2 * 3 + 2], Cell::Object(_)),
            "not object at (2,2)"
        );
        assert!(
            matches!(result[3 * 3 + 0], Cell::Object(_)),
            "object at (0,3)"
        );
        assert!(
            matches!(result[3 * 3 + 1], Cell::Object(_)),
            "object at (1,3)"
        );
        assert!(
            matches!(result[3 * 3 + 2], Cell::Object(_)),
            "object at (2,3)"
        );
    }
}
