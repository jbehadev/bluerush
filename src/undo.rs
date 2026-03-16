use bevy::prelude::*;

use crate::simulation::{Cell, Grid};

/// A before/after snapshot of a single cell, used to build undo/redo actions.
#[derive(Clone)]
pub struct CellChange {
    pub x: usize,
    pub y: usize,
    pub old: Cell,
    pub new: Cell,
}

/// A single undoable user action, consisting of one or more cell changes
/// accumulated during a mouse stroke.
pub struct Action {
    pub changes: Vec<CellChange>,
}

/// Undo/redo history for user edits. Capped at `MAX_UNDO` actions.
///
/// Changes are accumulated in a `pending` buffer while the mouse button is held,
/// then committed as a single `Action` on mouse release.
#[derive(Resource, Default)]
pub struct UndoStack {
    undo: Vec<Action>,
    redo: Vec<Action>,
    pending: Vec<CellChange>,
}

const MAX_UNDO: usize = 100;

impl UndoStack {
    /// Record a single cell change during the current stroke.
    /// Skips no-op changes where old == new.
    pub fn record(&mut self, x: usize, y: usize, old: Cell, new: Cell) {
        if old == new {
            return;
        }
        self.pending.push(CellChange { x, y, old, new });
    }

    /// Commit the current pending changes as one undoable action.
    /// Clears the redo stack (new action invalidates redo history).
    pub fn commit(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.pending);
        self.undo.push(Action { changes });
        self.redo.clear();
        // Cap at MAX_UNDO
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
    }

    /// Undo the most recent action, applying old cell values to the grid.
    pub fn undo(&mut self, grid: &mut Grid) {
        if let Some(action) = self.undo.pop() {
            for change in &action.changes {
                grid.set_cell(change.x, change.y, change.old.clone());
            }
            self.redo.push(action);
        }
    }

    /// Redo the most recently undone action, applying new cell values to the grid.
    pub fn redo(&mut self, grid: &mut Grid) {
        if let Some(action) = self.redo.pop() {
            for change in &action.changes {
                grid.set_cell(change.x, change.y, change.new.clone());
            }
            self.undo.push(action);
        }
    }

    /// Clear all undo/redo history and pending changes.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending.clear();
    }

    /// Returns `true` if there are uncommitted cell changes from the current stroke.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}
