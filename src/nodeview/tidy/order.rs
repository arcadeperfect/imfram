//! The order along each row that crosses the fewest wires: sweeps down
//! and up, each row sorted by the mean place of its neighbours in the row
//! before, then neighbouring pairs swapped while that uncrosses wires;
//! the best order seen is kept. (wobble's crossing reduction.)
//! Every wire here joins neighbouring rows (long ones were cut into
//! steps through stand-in vertices first). A wire's end counts where its
//! port is along the node, so sources line up over the inputs they feed.

use super::Link;

/// Down-and-up sweeps.
const SWEEPS: usize = 12;
/// Swap passes after each sweep.
const SWAPS: usize = 4;

/// `rows`: each row's vertices, left to right (reordered in place).
/// `up` / `down`: each vertex's neighbours in the row above / below.
pub fn reduce(rows: &mut [Vec<usize>], up: &[Vec<Link>], down: &[Vec<Link>]) {
    let mut best = rows.to_vec();
    let mut fewest = crossings(rows, down);
    for sweep in 0..SWEEPS {
        let going_down = sweep % 2 == 0;
        let order: Vec<usize> = if going_down { (1..rows.len()).collect() } else { (0..rows.len().saturating_sub(1)).rev().collect() };
        for r in order {
            let (fixed, near) = if going_down { (r - 1, up) } else { (r + 1, down) };
            let place = places(&rows[fixed], up.len());
            let mean = |v: usize, own: usize| -> f32 {
                let ns = &near[v];
                if ns.is_empty() {
                    own as f32
                } else {
                    ns.iter().map(|l| at(&place, l)).sum::<f32>() / ns.len() as f32
                }
            };
            let mut keyed: Vec<(f32, usize, usize)> = rows[r].iter().enumerate().map(|(i, &v)| (mean(v, i), i, v)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            rows[r] = keyed.into_iter().map(|k| k.2).collect();
        }
        for _ in 0..SWAPS {
            if !swap_pass(rows, up, down) {
                break;
            }
        }
        let now = crossings(rows, down);
        if now < fewest {
            fewest = now;
            best = rows.to_vec();
        }
    }
    rows.clone_from_slice(&best);
}

/// Where a wire's far end is along its row: its vertex's place, and its
/// port's place along that.
fn at(place: &[usize], l: &Link) -> f32 {
    place[l.n] as f32 + 0.9 * l.far
}

/// Each vertex's place in its row, indexed by vertex (of `total`; those
/// not in the row 0).
fn places(row: &[usize], total: usize) -> Vec<usize> {
    let mut at = vec![0usize; total];
    for (i, &v) in row.iter().enumerate() {
        at[v] = i;
    }
    at
}

/// Swap neighbouring pairs wherever that crosses fewer wires (above and
/// below together). True if any was swapped.
fn swap_pass(rows: &mut [Vec<usize>], up: &[Vec<Link>], down: &[Vec<Link>]) -> bool {
    let mut any = false;
    for r in 0..rows.len() {
        for i in 0..rows[r].len().saturating_sub(1) {
            let (a, b) = (rows[r][i], rows[r][i + 1]);
            let mut now = 0;
            let mut swapped = 0;
            for (near, other) in [(up, r.checked_sub(1)), (down, (r + 1 < rows.len()).then_some(r + 1))] {
                let Some(o) = other else { continue };
                let place = places(&rows[o], up.len());
                now += pair(&near[a], &near[b], &place);
                swapped += pair(&near[b], &near[a], &place);
            }
            if swapped < now {
                rows[r].swap(i, i + 1);
                any = true;
            }
        }
    }
    any
}

/// How many of `left`'s wires cross `right`'s, `left` being left of
/// `right` (the far ends' places given).
fn pair(left: &[Link], right: &[Link], place: &[usize]) -> usize {
    left.iter().map(|l| right.iter().filter(|r| at(place, r) < at(place, l)).count()).sum()
}

/// Every crossing between each row and the next.
pub fn crossings(rows: &[Vec<usize>], down: &[Vec<Link>]) -> usize {
    let mut total = 0;
    for r in 0..rows.len().saturating_sub(1) {
        let place = places(&rows[r + 1], down.len());
        let wires: Vec<(f32, f32)> = rows[r].iter().enumerate().flat_map(|(i, &v)| down[v].iter().map(move |l| (i as f32 + 0.9 * l.near, l))).map(|(i, l)| (i, at(&place, l))).collect();
        for (k, a) in wires.iter().enumerate() {
            total += wires[k + 1..].iter().filter(|b| (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1)).count();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crossed_pair_is_uncrossed() {
        let mut rows = vec![vec![0, 1], vec![2, 3]];
        let l = |n: usize| Link { n, far: 0.0, near: 0.0 };
        let up = vec![vec![], vec![], vec![l(1)], vec![l(0)]];
        let down = vec![vec![l(3)], vec![l(2)], vec![], vec![]];
        assert_eq!(crossings(&rows, &down), 1);
        reduce(&mut rows, &up, &down);
        assert_eq!(crossings(&rows, &down), 0);
    }
}
