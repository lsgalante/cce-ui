//! Undo/redo history: a snapshot stack any editing state can own.
//!
//! The toolkit deliberately does not define what an undoable step *is* —
//! that differs per app (a node graph, a palette, a KDL tree, a text
//! buffer). It defines the stack: `History<T>` holds snapshots of the
//! caller's own state type, one per recorded step, with the three rules
//! every undo system needs and every hand-rolled one gets subtly wrong:
//!
//! - **fork on new edit** — recording after an undo drops the redo branch;
//! - **one entry per gesture** — a drag or a typed run is one step, via
//!   [`History::begin_gesture`] (records lazily, so a gesture that never
//!   changes anything leaves nothing) and [`History::record_grouped`]
//!   (consecutive records in the same group keep only the first snapshot);
//! - **a cap** — the oldest entries fall off.
//!
//! Routing is the other half of the system and lives in the runner: the
//! `undo` / `redo` chords from `input.kdl` (cce-ui domain defaults
//! `ctrl+z` / `ctrl+shift+z`) go first to the focused widget
//! ([`ContextAction::Undo`](crate::widget::ContextAction) — a text box
//! undoes its own typing), then to the app's
//! [`Application::undo`](crate::engine::Application::undo) /
//! [`redo`](crate::engine::Application::redo). An app that owns a
//! project-wide history answers there; an app with several editing states
//! consults them in order and answers with the first that has something.
//!
//! Snapshots, not commands: the state types in this DE are small and
//! cloneable, and a snapshot restore is correct no matter what happened to
//! the state in between (an MCP edit, a reload) — a command's inverse is
//! not. Apps whose state is large can hold a diff type in `T` instead; the
//! stack does not care.

/// Default cap on undo depth.
pub const DEFAULT_LIMIT: usize = 256;

#[derive(Debug, Clone)]
pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
    /// A gesture's pre-state, held until the first change commits it.
    pending: Option<T>,
    /// The group of the last record, for coalescing typed runs.
    last_group: Option<u32>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> History<T> {
    pub fn new() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }

    pub fn with_limit(limit: usize) -> Self {
        History { undo: Vec::new(), redo: Vec::new(), limit: limit.max(1), pending: None, last_group: None }
    }

    /// Record `before` as the state the next undo returns to. Forks: the
    /// redo branch is dropped. Ends any coalescing group.
    pub fn record(&mut self, before: T) {
        self.last_group = None;
        self.push(before);
    }

    /// Record `before` unless the previous record was in the same `group`,
    /// in which case the earlier snapshot already covers this change and
    /// nothing is pushed. Typing "hello" with group per keystroke is one
    /// step; call [`break_group`](Self::break_group) when something else
    /// happens between two keystrokes (a cursor move) so the next one starts
    /// a fresh step.
    pub fn record_grouped(&mut self, before: T, group: u32) {
        if self.last_group == Some(group) && !self.undo.is_empty() {
            // Still one step. A redo branch cannot exist here: an undo
            // breaks the group, so a fresh record after it forks as usual.
            return;
        }
        self.push(before);
        self.last_group = Some(group);
    }

    /// End the current coalescing group: the next grouped record starts a
    /// new step even if it is in the same group.
    pub fn break_group(&mut self) {
        self.last_group = None;
    }

    /// Start a gesture (a drag): hold `before` without recording it. The
    /// first [`commit_gesture`](Self::commit_gesture) records it; a gesture
    /// that ends without one leaves no history entry.
    pub fn begin_gesture(&mut self, before: T) {
        self.pending = Some(before);
    }

    /// The gesture changed something: record its pre-state, once. Returns
    /// whether this call was the one that recorded it.
    pub fn commit_gesture(&mut self) -> bool {
        match self.pending.take() {
            Some(before) => {
                self.record(before);
                true
            }
            None => false,
        }
    }

    /// Drop a gesture that changed nothing (or was abandoned).
    pub fn cancel_gesture(&mut self) {
        self.pending = None;
    }

    pub fn in_gesture(&self) -> bool {
        self.pending.is_some()
    }

    /// Step back: returns the snapshot to restore, having filed `current`
    /// on the redo stack. `None` when there is nothing to undo — `current`
    /// is dropped in that case, so callers can pass a fresh clone.
    pub fn undo(&mut self, current: T) -> Option<T> {
        let target = self.undo.pop()?;
        self.redo.push(current);
        self.pending = None;
        self.last_group = None;
        Some(target)
    }

    /// Step forward: the counterpart of [`undo`](Self::undo).
    pub fn redo(&mut self, current: T) -> Option<T> {
        let target = self.redo.pop()?;
        self.undo.push(current);
        self.pending = None;
        self.last_group = None;
        Some(target)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Forget everything — a new document, a new editing session.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
        self.last_group = None;
    }

    fn push(&mut self, before: T) {
        self.undo.push(before);
        self.redo.clear();
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_walk_and_fork() {
        let mut h = History::new();
        let mut v = 0;
        for next in 1..=3 {
            h.record(v);
            v = next;
        }
        assert_eq!((h.undo_len(), h.redo_len()), (3, 0));
        v = h.undo(v).unwrap();
        assert_eq!(v, 2);
        v = h.undo(v).unwrap();
        assert_eq!(v, 1);
        assert_eq!((h.undo_len(), h.redo_len()), (1, 2));
        v = h.redo(v).unwrap();
        assert_eq!(v, 2);
        // A new edit after an undo drops the remaining redo branch.
        h.record(v);
        v = 10;
        assert_eq!((h.undo_len(), h.redo_len()), (3, 0));
        assert!(h.redo(v).is_none());
        v = h.undo(v).unwrap();
        assert_eq!(v, 2);
        assert_eq!(h.redo(v), Some(10));
    }

    #[test]
    fn gesture_records_once_and_only_if_committed() {
        let mut h: History<i32> = History::new();
        h.begin_gesture(0);
        assert!(h.in_gesture());
        h.cancel_gesture();
        assert!(!h.can_undo(), "an abandoned gesture leaves nothing");

        h.begin_gesture(0);
        assert!(h.commit_gesture());
        assert!(!h.commit_gesture(), "second motion is the same step");
        assert!(!h.commit_gesture());
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.undo(5), Some(0));
    }

    #[test]
    fn grouped_records_coalesce_until_broken() {
        let mut h: History<&str> = History::new();
        h.record_grouped("", 1);
        h.record_grouped("h", 1);
        h.record_grouped("he", 1);
        assert_eq!(h.undo_len(), 1, "a typed run is one step");
        h.record_grouped("hel", 2);
        assert_eq!(h.undo_len(), 2, "a different group starts a step");
        h.break_group();
        h.record_grouped("hel ", 2);
        assert_eq!(h.undo_len(), 3, "break_group splits the same group");
        assert_eq!(h.undo("hel w"), Some("hel "));
        // An undo ends the group too: the next grouped record is a fresh
        // step (and forks the redo branch).
        h.record_grouped("hel ", 2);
        assert_eq!((h.undo_len(), h.redo_len()), (3, 0));
    }

    #[test]
    fn limit_drops_the_oldest() {
        let mut h = History::with_limit(2);
        h.record(1);
        h.record(2);
        h.record(3);
        assert_eq!(h.undo_len(), 2);
        assert_eq!(h.undo(4), Some(3));
        assert_eq!(h.undo(3), Some(2));
        assert_eq!(h.undo(2), None);
    }

    #[test]
    fn clear_forgets_everything() {
        let mut h = History::new();
        h.record(1);
        h.begin_gesture(2);
        h.clear();
        assert!(!h.can_undo() && !h.can_redo() && !h.in_gesture());
    }
}
