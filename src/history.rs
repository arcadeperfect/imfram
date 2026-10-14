//! Snapshot undo. An edit burst (a slider drag, a wire, some typing) becomes
//! one step once it has settled, so undo steps match what the user did
//! rather than every frame a value moved.

use web_time::{Duration, Instant};

const LIMIT: usize = 200;
/// Quiet time (with no button held) that closes an edit burst.
pub const SETTLE: Duration = Duration::from_millis(250);

pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    /// The document as of the last step boundary.
    baseline: T,
    /// The document has moved on from `baseline`.
    pending: bool,
    last_edit: Instant,
}

impl<T: Clone> History<T> {
    pub fn new(doc: &T) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), baseline: doc.clone(), pending: false, last_edit: Instant::now() }
    }

    /// Forget everything: the document was replaced wholesale.
    pub fn reset(&mut self, doc: &T) {
        *self = Self::new(doc);
    }

    /// The document was edited this frame.
    pub fn touched(&mut self) {
        self.pending = true;
        self.last_edit = Instant::now();
    }

    /// An edit burst is open (not yet an undo step).
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// The open burst has gone quiet for long enough to close.
    pub fn settled(&self) -> bool {
        self.pending && self.last_edit.elapsed() >= SETTLE
    }

    pub fn can_undo(&self) -> bool {
        self.pending || !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.pending && !self.redo.is_empty()
    }

    /// Steps available: (undo, redo).
    pub fn depth(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    /// Close the open burst as one undo step.
    pub fn commit(&mut self, doc: &T) {
        if self.pending {
            self.undo.push(std::mem::replace(&mut self.baseline, doc.clone()));
            if self.undo.len() > LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.pending = false;
        }
    }

    /// Step back. An open burst counts as a step. True when `doc` changed.
    pub fn undo(&mut self, doc: &mut T) -> bool {
        self.commit(doc);
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(doc, prev));
        self.baseline = doc.clone();
        true
    }

    /// Step forward. True when `doc` changed.
    pub fn redo(&mut self, doc: &mut T) -> bool {
        if self.pending {
            return false; // a new edit has already replaced the redo branch
        }
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(doc, next));
        self.baseline = doc.clone();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Edit the document and close the burst, as a settled edit would.
    fn edit(h: &mut History<i32>, doc: &mut i32, value: i32) {
        *doc = value;
        h.touched();
        h.commit(doc);
    }

    #[test]
    fn undo_and_redo_walk_the_steps() {
        let mut doc = 0;
        let mut h = History::new(&doc);
        edit(&mut h, &mut doc, 1);
        edit(&mut h, &mut doc, 2);
        assert!(h.undo(&mut doc));
        assert_eq!(doc, 1);
        assert!(h.undo(&mut doc));
        assert_eq!(doc, 0);
        assert!(!h.undo(&mut doc), "nothing left");
        assert!(h.redo(&mut doc));
        assert!(h.redo(&mut doc));
        assert_eq!(doc, 2);
        assert!(!h.redo(&mut doc));
    }

    #[test]
    fn a_burst_is_one_step() {
        let mut doc = 0;
        let mut h = History::new(&doc);
        for v in 1..=5 {
            doc = v; // a drag: many frames, no commit yet
            h.touched();
        }
        assert!(h.pending() && h.can_undo());
        assert!(h.undo(&mut doc), "undo closes the open burst first");
        assert_eq!(doc, 0);
        assert!(h.redo(&mut doc));
        assert_eq!(doc, 5);
    }

    #[test]
    fn a_new_edit_drops_the_redo_branch() {
        let mut doc = 0;
        let mut h = History::new(&doc);
        edit(&mut h, &mut doc, 1);
        h.undo(&mut doc);
        assert!(h.can_redo());
        doc = 7;
        h.touched();
        assert!(!h.redo(&mut doc), "an open edit blocks redo");
        h.commit(&doc);
        assert!(!h.can_redo());
        assert_eq!(doc, 7);
    }

    #[test]
    fn reset_forgets_everything() {
        let mut doc = 0;
        let mut h = History::new(&doc);
        edit(&mut h, &mut doc, 1);
        h.reset(&doc);
        assert_eq!(h.depth(), (0, 0));
        assert!(!h.undo(&mut doc));
        assert_eq!(doc, 1);
    }
}
