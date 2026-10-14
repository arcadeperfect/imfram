//! Where along its row each vertex goes (its centre's x): rows packed,
//! then passes down and up in which each vertex, most-wired first, moves
//! toward the median x of its neighbours in the row before, never past a
//! neighbour that matters as much, pushing aside those that matter less.
//! Stand-in vertices (a long wire's steps) matter most, so long wires
//! come out straight. (wobble's priority placement.)

use super::Link;

/// Room between neighbours' edges: real nodes, and a long wire's steps.
pub const COL_GAP: f32 = 36.0;
pub const CHAN_GAP: f32 = 24.0;
/// More room between two nodes nothing feeds, and between two that share
/// nothing that feeds them (they are not one family).
const ROOTS: f32 = 1.4;
const STRANGERS: f32 = 1.6;
/// Passes, alternating down and up.
const PASSES: usize = 16;

/// What placement needs of a vertex.
pub struct Vertex {
    pub width: f32,
    /// A node, not a step of a long wire.
    pub real: bool,
}

/// The x of every vertex's centre.
pub fn xs(rows: &[Vec<usize>], v: &[Vertex], up: &[Vec<Link>], down: &[Vec<Link>]) -> Vec<f32> {
    let gap = |a: usize, b: usize| {
        let g = |i: usize| if v[i].real { COL_GAP } else { CHAN_GAP };
        let zone = if v[a].real && v[b].real {
            if up[a].is_empty() && up[b].is_empty() {
                ROOTS
            } else if !up[a].iter().any(|p| up[b].iter().any(|q| q.n == p.n)) {
                STRANGERS
            } else {
                1.0
            }
        } else {
            1.0
        };
        0.5 * (v[a].width + v[b].width) + 0.5 * (g(a) + g(b)) * zone
    };
    let priority = |i: usize| (up[i].len() + down[i].len()) as u64 + if v[i].real { 0 } else { 1_000_000 };
    let mut x = vec![0.0f32; v.len()];
    for row in rows {
        let mut at = 0.0;
        for (k, &i) in row.iter().enumerate() {
            if k > 0 {
                at += gap(row[k - 1], i);
            }
            x[i] = at;
        }
        for &i in row {
            x[i] -= 0.5 * at;
        }
    }
    for pass in 0..PASSES {
        let going_down = pass % 2 == 0;
        let order: Vec<usize> = if going_down { (0..rows.len()).collect() } else { (0..rows.len()).rev().collect() };
        for r in order {
            let row = &rows[r];
            let mut by_priority: Vec<usize> = (0..row.len()).collect();
            by_priority.sort_by_key(|&k| std::cmp::Reverse(priority(row[k])));
            for k in by_priority {
                let i = row[k];
                let near = if going_down { &up[i] } else { &down[i] };
                let near: Vec<&Link> = if near.is_empty() { up[i].iter().chain(&down[i]).collect() } else { near.iter().collect() };
                if near.is_empty() {
                    continue;
                }
                // (Where this end's port would sit under, or over, the far one's.)
                let mut at: Vec<f32> = near.iter().map(|l| x[l.n] + l.far * v[l.n].width - l.near * v[i].width).collect();
                at.sort_by(f32::total_cmp);
                let target = if at.len() % 2 == 1 { at[at.len() / 2] } else { 0.5 * (at[at.len() / 2 - 1] + at[at.len() / 2]) };
                place(&mut x, row, k, target, &gap, &priority);
            }
        }
    }
    x
}

/// Move `row[k]` toward `target`: no further than leaves room for those
/// between it and the nearest neighbour as important as it, pushing the
/// less important along.
fn place(x: &mut [f32], row: &[usize], k: usize, target: f32, gap: &impl Fn(usize, usize) -> f32, priority: &impl Fn(usize) -> u64) {
    let me = row[k];
    if target < x[me] {
        let mut room = 0.0;
        let mut limit = f32::NEG_INFINITY;
        for j in (0..k).rev() {
            room += gap(row[j], row[j + 1]);
            if priority(row[j]) >= priority(me) {
                limit = x[row[j]] + room;
                break;
            }
        }
        x[me] = target.max(limit).min(x[me]);
        for j in (0..k).rev() {
            x[row[j]] = x[row[j]].min(x[row[j + 1]] - gap(row[j], row[j + 1]));
        }
    } else if target > x[me] {
        let mut room = 0.0;
        let mut limit = f32::INFINITY;
        for j in k + 1..row.len() {
            room += gap(row[j - 1], row[j]);
            if priority(row[j]) >= priority(me) {
                limit = x[row[j]] - room;
                break;
            }
        }
        x[me] = target.min(limit).max(x[me]);
        for j in k + 1..row.len() {
            x[row[j]] = x[row[j]].max(x[row[j - 1]] + gap(row[j - 1], row[j]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(width: f32) -> Vertex {
        Vertex { width, real: true }
    }

    #[test]
    fn neighbours_never_overlap_and_a_chain_comes_out_straight() {
        let rows = vec![vec![0, 1], vec![2]];
        let v = [node(200.0), node(200.0), node(200.0)];
        let l = |n: usize| Link { n, far: 0.0, near: 0.0 };
        let up = vec![vec![], vec![], vec![l(0), l(1)]];
        let down = vec![vec![l(2)], vec![l(2)], vec![]];
        let x = xs(&rows, &v, &up, &down);
        assert!(x[1] - x[0] >= 200.0 + COL_GAP, "{x:?}");
        assert!((x[2] - 0.5 * (x[0] + x[1])).abs() < 1.0, "the sink sits under the middle of its two: {x:?}");
        let rows = vec![vec![0], vec![1], vec![2]];
        let v = [node(200.0), node(120.0), node(200.0)];
        let up = vec![vec![], vec![l(0)], vec![l(1)]];
        let down = vec![vec![l(1)], vec![l(2)], vec![]];
        let x = xs(&rows, &v, &up, &down);
        assert!((x[0] - x[1]).abs() < 1e-3 && (x[1] - x[2]).abs() < 1e-3, "{x:?}");
    }
}
