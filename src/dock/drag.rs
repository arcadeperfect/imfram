//! Dragging a tab out of its window: past the window's edge, the window
//! under the pointer (another of ours) lights up; let go over it and the
//! tab docks there, over the desktop and it becomes a window of its own
//! where it was let go. (The tab itself cannot be drawn between windows:
//! only the target lights up.) Screen points are physical pixels.

use super::tree::Drop;
use super::Ask;

/// A tab being dragged out of window `from`, and the window it is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag<T> {
    pub tab: T,
    pub from: usize,
    pub over: Option<usize>,
    /// The pointer on the screen (physical pixels).
    pub at: Option<[f64; 2]>,
    /// Where the window it is over would put it (that window's frame says).
    pub drop: Option<Drop>,
}

/// Where a window is on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl Frame {
    fn contains(&self, p: [f64; 2]) -> bool {
        (self.min[0]..self.max[0]).contains(&p[0]) && (self.min[1]..self.max[1]).contains(&p[1])
    }

    fn area(&self) -> f64 {
        (self.max[0] - self.min[0]) * (self.max[1] - self.min[1])
    }
}

/// The window under screen point `p`, not counting `not` (where the drag
/// came from). Where windows overlap, the smallest: the likeliest to be
/// the one on top.
pub fn under(windows: &[(usize, Frame)], p: [f64; 2], not: usize) -> Option<usize> {
    windows
        .iter()
        .filter(|(i, f)| *i != not && f.contains(p))
        .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
        .map(|(i, _)| *i)
}

/// What letting go of `tab` outside its window means: docked in the
/// window it is over, or a window of its own at `at`.
pub fn drop<T>(tab: T, over: Option<usize>, into: Option<Drop>, at: [f64; 2]) -> Ask<T> {
    match over {
        Some(w) => Ask::Move(tab, w, into),
        None => Ask::DetachAt(tab, at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f64, y: f64, w: f64, h: f64) -> Frame {
        Frame { min: [x, y], max: [x + w, y + h] }
    }

    #[test]
    fn the_window_under_the_pointer_is_found_but_never_the_one_it_came_from() {
        let ws = [(0, frame(0.0, 0.0, 800.0, 600.0)), (1, frame(900.0, 0.0, 400.0, 400.0))];
        assert_eq!(under(&ws, [1000.0, 100.0], 0), Some(1));
        assert_eq!(under(&ws, [100.0, 100.0], 0), None, "its own window does not count");
        assert_eq!(under(&ws, [850.0, 100.0], 0), None, "the desktop between them");
    }

    #[test]
    fn where_windows_overlap_the_smaller_is_taken() {
        let ws = [(0, frame(0.0, 0.0, 800.0, 600.0)), (1, frame(100.0, 100.0, 200.0, 200.0)), (2, frame(900.0, 0.0, 10.0, 10.0))];
        assert_eq!(under(&ws, [150.0, 150.0], 2), Some(1));
    }

    #[test]
    fn let_go_over_a_window_it_docks_there_over_the_desktop_it_is_its_own() {
        assert_eq!(drop("objects", Some(3), None, [5.0, 5.0]), Ask::Move("objects", 3, None));
        assert_eq!(drop("objects", None, None, [5.0, 5.0]), Ask::DetachAt("objects", [5.0, 5.0]));
    }
}
