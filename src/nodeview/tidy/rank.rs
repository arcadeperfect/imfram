//! Rows: which row (rank) each node goes in, top to bottom. Longest path
//! from the sources, then each node pulled to the median of where its
//! wires want it (a source drops to just above what it feeds), then the
//! empty rows taken out. (wobble's ranking.)

/// How many times the median pull is gone over.
const TIGHTEN: usize = 4;

/// The rank of each of `n` nodes, given its parents (what feeds it) and
/// children (what it feeds). Nodes in a loop go in a row of their own,
/// under the rest.
pub fn ranks(n: usize, parents: &[Vec<usize>], children: &[Vec<usize>]) -> Vec<usize> {
    let mut rank = vec![0usize; n];
    let mut waiting: Vec<usize> = (0..n).map(|i| parents[i].len()).collect();
    let mut ready: Vec<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
    let mut placed = vec![false; n];
    while let Some(v) = ready.pop() {
        placed[v] = true;
        for &c in &children[v] {
            rank[c] = rank[c].max(rank[v] + 1);
            waiting[c] -= 1;
            if waiting[c] == 0 {
                ready.push(c);
            }
        }
    }
    let deepest = rank.iter().zip(&placed).filter(|(_, p)| **p).map(|(r, _)| *r).max().unwrap_or(0);
    for v in (0..n).filter(|&v| !placed[v]) {
        rank[v] = deepest + 1;
    }
    for _ in 0..TIGHTEN {
        let mut moved = false;
        for v in 0..n {
            let lo = parents[v].iter().map(|&p| rank[p] + 1).max().unwrap_or(0);
            let hi = children[v].iter().map(|&c| rank[c].saturating_sub(1)).min().unwrap_or(usize::MAX);
            if lo > hi {
                continue;
            }
            let mut wanted: Vec<usize> = parents[v].iter().map(|&p| rank[p] + 1).chain(children[v].iter().map(|&c| rank[c].saturating_sub(1))).collect();
            if wanted.is_empty() {
                continue;
            }
            wanted.sort_unstable();
            let to = wanted[wanted.len() / 2].clamp(lo, hi);
            if to != rank[v] {
                rank[v] = to;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // Close up the rows nothing is in.
    let mut used: Vec<usize> = rank.clone();
    used.sort_unstable();
    used.dedup();
    rank.iter().map(|r| used.binary_search(r).expect("a used rank")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wired(n: usize, edges: &[(usize, usize)]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        let (mut p, mut c) = (vec![Vec::new(); n], vec![Vec::new(); n]);
        for &(a, b) in edges {
            p[b].push(a);
            c[a].push(b);
        }
        (p, c)
    }

    #[test]
    fn a_chain_goes_one_row_each_and_a_source_drops_to_just_above_what_it_feeds() {
        let (p, c) = wired(3, &[(0, 1), (1, 2)]);
        assert_eq!(ranks(3, &p, &c), [0, 1, 2]);
        let (p, c) = wired(5, &[(0, 1), (1, 2), (2, 3), (4, 3)]);
        let r = ranks(5, &p, &c);
        assert_eq!(r[4], r[3] - 1);
        let (p, c) = wired(2, &[(0, 1), (1, 0)]);
        assert_eq!(ranks(2, &p, &c).len(), 2, "a loop does not hang it");
    }
}
