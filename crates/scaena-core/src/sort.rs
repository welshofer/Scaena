//! Stable sorts that share one compiled merge sort.
//!
//! `slice::sort_by` compiles a whole sort for each element type and comparator it is called
//! with: some 10 KB of WASM a call site, which in the editor's module came to 470 KB, and took
//! it past SPEC §15's budget. These sort a permutation of indices instead, by one merge sort
//! that calls the comparator through `dyn`, so a call site adds its comparator and the
//! permutation applied in place. A stable sort's result is fixed by its comparator, ties kept in
//! the order they came, so for any total order these give what `sort_by` gives.

use std::cmp::Ordering;

/// `v` in the order `cmp` gives, stably: what `v.sort_by(cmp)` makes of it.
pub fn by<T>(v: &mut [T], mut cmp: impl FnMut(&T, &T) -> Ordering) {
    if v.len() < 2 {
        return;
    }
    let order = permutation(v.len(), &mut |i, j| cmp(&v[i], &v[j]));
    permute(v, &order);
}

/// `v` in the order of the key `key` gives each element, stably: what `v.sort_by_key(key)`
/// makes of it.
pub fn by_key<T, K: Ord>(v: &mut [T], mut key: impl FnMut(&T) -> K) {
    by(v, |a, b| key(a).cmp(&key(b)));
}

/// `v` in its own order, stably: what `v.sort()` makes of it.
pub fn sort<T: Ord>(v: &mut [T]) {
    by(v, T::cmp);
}

/// The indices `0..n` in the order `cmp` gives them, ties in the order they come: a bottom-up
/// merge sort, compiled once for every caller.
fn permutation(n: usize, cmp: &mut dyn FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut from: Vec<usize> = (0..n).collect();
    let mut to = vec![0; n];
    let mut width = 1;
    while width < n {
        for start in (0..n).step_by(2 * width) {
            let (mid, end) = ((start + width).min(n), (start + 2 * width).min(n));
            let (mut l, mut r) = (start, mid);
            for slot in &mut to[start..end] {
                // The right run's next goes first only when it is less: ties keep their order.
                let right = r < end && (l == mid || cmp(from[r], from[l]) == Ordering::Less);
                let i = if right { &mut r } else { &mut l };
                *slot = from[*i];
                *i += 1;
            }
        }
        std::mem::swap(&mut from, &mut to);
        width *= 2;
    }
    from
}

/// `v` rearranged so that position `k` holds what was at `order[k]`, by swaps.
fn permute<T>(v: &mut [T], order: &[usize]) {
    // Where each element is now, and which element each position holds, as the swaps move them.
    let mut at: Vec<usize> = (0..v.len()).collect();
    let mut holds = at.clone();
    for (k, &want) in order.iter().enumerate() {
        // The positions before `k` hold what they will, so `want` stands at `k` or after it.
        let p = at[want];
        if p != k {
            v.swap(k, p);
            let moved = holds[k];
            holds.swap(k, p);
            (at[want], at[moved]) = (k, p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few hundred pseudo-random keys with many ties, as a fixed sequence.
    fn keys(n: usize, range: u64, mut seed: u64) -> Vec<u64> {
        (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 33) % range
            })
            .collect()
    }

    /// Each element is its key and where it started: a stable sort's every tie shows.
    #[test]
    fn the_order_is_the_one_sort_by_gives() {
        for (n, range, seed) in [(0, 1, 1), (1, 1, 2), (2, 1, 3), (7, 3, 4), (64, 5, 5), (300, 17, 6), (1025, 1000, 7)]
        {
            let tagged: Vec<(u64, usize)> = keys(n, range, seed).into_iter().zip(0..).collect();
            let mut ours = tagged.clone();
            by(&mut ours, |a, b| a.0.cmp(&b.0));
            let mut theirs = tagged.clone();
            theirs.sort_by_key(|a| a.0);
            assert_eq!(ours, theirs, "{n} keys in 0..{range}");
            let mut ours = tagged.clone();
            by_key(&mut ours, |&(k, _)| std::cmp::Reverse(k));
            let mut theirs = tagged;
            theirs.sort_by_key(|&(k, _)| std::cmp::Reverse(k));
            assert_eq!(ours, theirs, "{n} keys in 0..{range}, reversed");
        }
    }

    /// What does not copy moves by swaps, and comes out whole.
    #[test]
    fn owned_elements_come_out_whole() {
        let mut words: Vec<String> = ["pear", "fig", "apple", "fig", "kiwi", "date"].map(String::from).to_vec();
        sort(&mut words);
        assert_eq!(words, ["apple", "date", "fig", "fig", "kiwi", "pear"]);
        let mut floats = [2.5, -0.0, 1.0, 0.0, -3.0];
        by(&mut floats, f64::total_cmp);
        assert_eq!(floats.map(f64::to_bits), [-3.0f64, -0.0, 0.0, 1.0, 2.5].map(f64::to_bits));
    }
}
