//! The mod-subset space as an INDEX RANGE — `nth(i)` instead of a walk.
//!
//! The enumeration used to be a depth-first descent (`enumerate_rec`), which
//! is fine when it runs to completion and disastrous when it does not: what a
//! cut leaves behind is a lexicographic prefix — builds made of the first few
//! pool entries plus one varying tail — rather than a sample. Measured on a
//! 22-mod pool, the complete walk carries Heat in 2.77% of its candidates and
//! the first 3,000 of a 60-mod pool carry it in 0% (docs/OPTIMIZER.md). A
//! browser can afford ~10⁴ evaluations against a space of ~10⁹, so being cut
//! is the NORMAL case, and an enumeration whose truncation is biased is not
//! usable at that ratio.
//!
//! Indexing fixes it at the root, and it does so without a second code path:
//!
//! - a **full sweep** is `0..len()`, and produces exactly what the walk did;
//! - a **sample** is any set of indices, and a uniform one is unbiased by
//!   construction;
//! - **coverage** is `tried / len()`, an exact number rather than a hope;
//! - a scope small enough to exhaust simply *is* exhausted by the sweep, so
//!   "small" and "large" stop being two behaviours (user, 2026-08-03: one
//!   path, rigour over convenience).
//!
//! **Family exclusivity is rejected, not indexed.** Folding mutually
//! exclusive families into the index is exact but intricate; rejecting the
//! collisions costs one pass over a subset that is already built. Measured
//! over the four shipped pools, family-legal subsets are **79–85%** of
//! C(n, 8), so rejection costs ~25% of the walk — against an evaluation that
//! costs a full simulated engagement, that is nothing.

/// A subset of the pool as an index range: `required` ∪ `extra` choices.
#[derive(Debug, Clone)]
pub struct SubsetSpace {
    /// Pool indices that may be chosen freely, in pool order.
    choosable: Vec<usize>,
    /// Family tag per `choosable` entry, interned to a small id.
    fam: Vec<Option<u32>>,
    /// Pool indices present in EVERY subset (the request's required mods).
    required: Vec<usize>,
    /// Family tag per `required` entry, aligned. A choosable sharing one can
    /// never join them.
    req_fam: Vec<Option<u32>>,
    /// Extra picks on top of `required`, inclusive.
    min_extra: usize,
    /// `cum[j]` = number of indices used by extra-counts `min_extra..=min_extra+j`.
    cum: Vec<u128>,
    /// `binom[a][b]` = C(a, b), for a ≤ choosable.len(), b ≤ max_extra.
    binom: Vec<Vec<u128>>,
}

impl SubsetSpace {
    /// `usable` and `required` are POOL indices; `min`/`max` are total subset
    /// sizes (required included), matching the enumeration's own parameters.
    /// A required mod listed in `usable` is not offered twice.
    pub fn new(
        families: &[Option<&'static str>],
        usable: &[usize],
        required: &[usize],
        min: usize,
        max: usize,
    ) -> Self {
        let mut interned: Vec<&'static str> = Vec::new();
        let mut id_of = |f: Option<&'static str>| -> Option<u32> {
            let f = f?;
            Some(match interned.iter().position(|x| *x == f) {
                Some(i) => i as u32,
                None => {
                    interned.push(f);
                    (interned.len() - 1) as u32
                }
            })
        };
        let req_fam: Vec<Option<u32>> = required.iter().map(|&i| id_of(families[i])).collect();
        let choosable: Vec<usize> = usable
            .iter()
            .copied()
            .filter(|i| !required.contains(i))
            .collect();
        let fam: Vec<Option<u32>> = choosable.iter().map(|&i| id_of(families[i])).collect();

        let min_extra = min.saturating_sub(required.len());
        // An `extra` count beyond what is choosable indexes nothing; clamping
        // here is what keeps `cum` free of zero-width tails.
        let max_extra = max.saturating_sub(required.len()).min(choosable.len());
        let n = choosable.len();
        // Pascal's triangle, wide enough for the largest pick count. C(70, 9)
        // is ~1.2e11, so u128 is headroom rather than necessity — but the
        // colex unranker sums these, and an overflow there would silently
        // return the wrong subset.
        let binom = {
            let mut b = vec![vec![0u128; max_extra + 1]; n + 1];
            b[0][0] = 1;
            for a in 1..=n {
                let prev = b[a - 1].clone();
                let row = &mut b[a];
                row[0] = 1;
                for k in 1..=max_extra.min(a) {
                    row[k] = prev[k - 1] + if k < a { prev[k] } else { 0 };
                }
            }
            b
        };
        let mut cum = Vec::new();
        let mut total = 0u128;
        if min_extra <= max_extra {
            for c in &binom[n][min_extra..=max_extra] {
                total += c;
                cum.push(total);
            }
        }
        SubsetSpace {
            choosable,
            fam,
            required: required.to_vec(),
            req_fam,
            min_extra,
            cum,
            binom,
        }
    }

    /// How many indices the space holds. Family-illegal ones are INCLUDED —
    /// they are positions the sweep visits and rejects, and reporting coverage
    /// against the number actually visited is what keeps it an exact ratio
    /// rather than an estimate of a count nobody computes.
    pub fn len(&self) -> u128 {
        self.cum.last().copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pool indices a subset may pick FREELY (required excluded) — the
    /// alphabet a neighbourhood move draws from.
    pub fn choosable(&self) -> &[usize] {
        &self.choosable
    }

    /// Pool indices in every subset.
    pub fn required(&self) -> &[usize] {
        &self.required
    }

    /// Legal subset sizes, required included.
    pub fn sizes(&self) -> std::ops::RangeInclusive<usize> {
        let lo = self.required.len() + self.min_extra;
        let hi = self.required.len() + self.cum.len().saturating_sub(1) + self.min_extra;
        lo..=hi
    }

    /// Is this a legal subset of THIS space — right size, no mutually
    /// exclusive pair, everything drawn from the scope? A neighbourhood move
    /// builds subsets directly rather than through an index, so it needs the
    /// same verdict `nth` reaches on the way.
    pub fn legal(&self, subset: &[usize]) -> bool {
        if !self.sizes().contains(&subset.len()) {
            return false;
        }
        if !self.required.iter().all(|r| subset.contains(r)) {
            return false;
        }
        let mut fams: Vec<u32> = Vec::with_capacity(subset.len());
        for &i in subset {
            let f = match self.required.iter().position(|&r| r == i) {
                Some(p) => self.req_fam[p],
                None => match self.choosable.iter().position(|&c| c == i) {
                    Some(p) => self.fam[p],
                    None => return false, // not in this scope at all
                },
            };
            if let Some(f) = f {
                if fams.contains(&f) {
                    return false;
                }
                fams.push(f);
            }
        }
        true
    }

    /// The subset at `i`, written into `out` (pool indices, required first).
    /// `false` = this index is a family collision and has no subset; the
    /// caller skips it. Indices ≥ [`len`] also return `false`.
    pub fn nth(&self, i: u128, out: &mut Vec<usize>) -> bool {
        let Some(pos) = self.cum.iter().position(|&c| i < c) else {
            return false;
        };
        let k = self.min_extra + pos;
        let base = if pos == 0 { 0 } else { self.cum[pos - 1] };
        let mut rank = i - base;

        out.clear();
        out.extend_from_slice(&self.required);
        // COLEX unranking: the r-th k-combination {c_1 < … < c_k} of 0..n is
        // the one with r = Σ C(c_j, j). Taking j from k down to 1 and picking
        // the largest c with C(c, j) ≤ r is exact and needs no search over the
        // whole space. Colex rather than lex because the digits come out
        // independently of n, which is what makes this O(k log n).
        let mut seen_fams: Vec<u32> = self.req_fam.iter().flatten().copied().collect();
        for j in (1..=k).rev() {
            // Largest c with C(c, j) ≤ rank, by binary search on the column.
            let (mut lo, mut hi) = (j - 1, self.choosable.len() - 1);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if self.binom[mid][j] <= rank {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            let c = lo;
            rank -= self.binom[c][j];
            if let Some(f) = self.fam[c] {
                if seen_fams.contains(&f) {
                    return false; // mutually exclusive with something already in
                }
                seen_fams.push(f);
            }
            out.push(self.choosable[c]);
        }
        // ASCENDING pool order, which is the order the depth-first walk built
        // its subsets in. `expand_one` preserves the subset's internal order
        // inside each element group, so matching it here makes the search and
        // the walk produce BIT-IDENTICAL candidates for the same subset —
        // same `ordered`, same dedup representative. That is what lets a
        // result be matched back by identity (the grader does exactly that)
        // instead of by a resolved vector nobody wants to compare on.
        out.sort_unstable();
        true
    }
}

/// A pseudorandom BIJECTION on `0..n` — the index order the search walks.
///
/// This is what makes one traversal answer both scopes. Walking `k = 0..n` and
/// visiting `at(k)` visits every index exactly once, so a scope the budget can
/// finish IS exhausted; and because the order is pseudorandom, stopping early
/// leaves a uniform sample WITHOUT REPLACEMENT rather than a prefix of the
/// pool's alphabet. There is no sampling mode to switch into and no
/// "exhaustive mode" to fall back to — the same loop is both, and which one it
/// turned out to be is just whether it reached the end.
///
/// The construction is a small Feistel network over the smallest even bit
/// width that covers `n`, with CYCLE-WALKING back into range: a Feistel round
/// is a bijection on `0..2^bits` for any round function, and re-applying it
/// while the output is out of range keeps it a bijection on `0..n` (the
/// out-of-range points form cycles that are skipped over). Expected walks are
/// under 2 because `2^bits < 4n`. Four rounds is far more mixing than a build
/// search needs; the point is only that adjacent indices land far apart.
#[derive(Debug, Clone)]
pub struct Shuffle {
    n: u128,
    half: u32,
    mask: u64,
    seed: u64,
}

impl Shuffle {
    pub fn new(n: u128, seed: u64) -> Self {
        let mut bits = 0u32;
        while (1u128 << bits) < n.max(1) {
            bits += 1;
        }
        if bits % 2 == 1 {
            bits += 1;
        }
        let half = (bits / 2).max(1);
        Shuffle { n, half, mask: (1u64 << half) - 1, seed }
    }

    fn round_fn(&self, r: u64, i: u64) -> u64 {
        // splitmix64 finalizer — cheap, and the mixing quality only has to beat
        // "adjacent indices stay adjacent".
        let mut z = r
            ^ (i.wrapping_mul(0x9E37_79B9_7F4A_7C15))
            ^ self.seed.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) & self.mask
    }

    fn feistel(&self, x: u64) -> u64 {
        let (mut l, mut r) = (x >> self.half, x & self.mask);
        for round in 0..4u64 {
            let f = self.round_fn(round, r);
            let next = l ^ f;
            l = r;
            r = next;
        }
        (l << self.half) | r
    }

    /// The `k`-th index of the shuffled order. `k` must be `< n`.
    pub fn at(&self, k: u128) -> u128 {
        if self.n <= 1 {
            return 0;
        }
        let mut x = k as u64;
        loop {
            x = self.feistel(x);
            if u128::from(x) < self.n {
                return u128::from(x);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fams(spec: &[Option<&'static str>]) -> Vec<Option<&'static str>> {
        spec.to_vec()
    }

    /// A sweep of the index space must produce EXACTLY the family-legal
    /// subsets — no duplicates, nothing missing. Brute-forced against every
    /// subset of a small pool, so the reference is not another copy of the
    /// same idea.
    #[test]
    fn a_sweep_is_every_family_legal_subset_exactly_once() {
        let f = fams(&[
            None,
            Some("bane"),
            Some("bane"),
            None,
            Some("serration"),
            Some("serration"),
            None,
            None,
        ]);
        let usable: Vec<usize> = (0..f.len()).collect();
        for (min, max) in [(1, 3), (2, 4), (0, 8), (3, 3)] {
            let s = SubsetSpace::new(&f, &usable, &[], min, max);
            let mut got: Vec<Vec<usize>> = Vec::new();
            let mut buf = Vec::new();
            for i in 0..s.len() {
                if s.nth(i, &mut buf) {
                    let mut v = buf.clone();
                    v.sort_unstable();
                    got.push(v);
                }
            }
            let before = got.len();
            got.sort();
            got.dedup();
            assert_eq!(before, got.len(), "the sweep repeated a subset ({min}..={max})");

            // Brute force: every bitmask, filtered the way the space claims to.
            let mut want: Vec<Vec<usize>> = Vec::new();
            for m in 0u32..(1 << f.len()) {
                let v: Vec<usize> = (0..f.len()).filter(|b| m >> b & 1 == 1).collect();
                if v.len() < min || v.len() > max {
                    continue;
                }
                let mut seen: Vec<&str> = Vec::new();
                if v.iter().any(|&i| match f[i] {
                    Some(x) if seen.contains(&x) => true,
                    Some(x) => {
                        seen.push(x);
                        false
                    }
                    None => false,
                }) {
                    continue;
                }
                want.push(v);
            }
            want.sort();
            assert_eq!(got, want, "index sweep != brute force for {min}..={max}");
        }
    }

    /// Required mods are in every subset and never offered a second time.
    #[test]
    fn required_mods_ride_along_and_are_not_re_offered() {
        let f = fams(&[None, Some("bane"), Some("bane"), None, None]);
        let usable: Vec<usize> = (0..5).collect();
        let s = SubsetSpace::new(&f, &usable, &[1], 2, 3);
        let mut buf = Vec::new();
        let mut n = 0;
        for i in 0..s.len() {
            if s.nth(i, &mut buf) {
                assert!(buf.contains(&1), "required mod missing from {buf:?}");
                assert_eq!(
                    buf.iter().filter(|&&x| x == 1).count(),
                    1,
                    "required mod offered twice in {buf:?}"
                );
                // Index 2 shares "bane" with the required index 1.
                assert!(!buf.contains(&2), "family collision with a required mod: {buf:?}");
                n += 1;
            }
        }
        assert!(n > 0, "the space produced nothing");
    }

    /// The count is the sum of the binomials it claims to index — the property
    /// that makes "explored X of N" an exact statement rather than an estimate.
    #[test]
    fn the_length_is_the_sum_of_the_binomials() {
        let f: Vec<Option<&'static str>> = vec![None; 20];
        let usable: Vec<usize> = (0..20).collect();
        let s = SubsetSpace::new(&f, &usable, &[], 1, 8);
        let c = |n: u128, k: u32| (0..k).fold(1u128, |a, i| a * (n - i as u128) / (i as u128 + 1));
        let want: u128 = (1..=8).map(|k| c(20, k)).sum();
        assert_eq!(s.len(), want);
    }

    /// A scope whose required mods already fill the build indexes exactly one
    /// subset — the empty extra choice — rather than nothing.
    #[test]
    fn a_fully_required_build_is_one_index_not_none() {
        let f: Vec<Option<&'static str>> = vec![None; 6];
        let usable: Vec<usize> = (0..6).collect();
        let s = SubsetSpace::new(&f, &usable, &[0, 1, 2], 3, 3);
        assert_eq!(s.len(), 1);
        let mut buf = Vec::new();
        assert!(s.nth(0, &mut buf));
        buf.sort_unstable();
        assert_eq!(buf, vec![0, 1, 2]);
    }
    /// The shuffled order must be a BIJECTION — every index exactly once. If
    /// it were not, a "full sweep" would silently skip builds while reporting
    /// 100% coverage, which is the failure this whole module exists to end.
    #[test]
    fn the_shuffled_order_visits_every_index_exactly_once() {
        for n in [1u128, 2, 3, 7, 16, 17, 100, 1000, 4097] {
            let sh = Shuffle::new(n, 0xC0FFEE);
            let mut seen = vec![false; n as usize];
            for k in 0..n {
                let x = sh.at(k);
                assert!(x < n, "index {x} out of range for n={n}");
                assert!(!seen[x as usize], "index {x} visited twice for n={n}");
                seen[x as usize] = true;
            }
            assert!(seen.iter().all(|&b| b), "n={n} left indices unvisited");
        }
    }

    /// ...and it must actually SHUFFLE. A prefix of the order is the sample a
    /// cut-short search keeps, so if the order tracked the natural one, the
    /// sample would be the same corner the depth-first walk left behind.
    #[test]
    fn a_prefix_of_the_order_is_spread_over_the_whole_space() {
        let n = 100_000u128;
        let sh = Shuffle::new(n, 7);
        // Ten buckets; a prefix of 1,000 should land roughly 100 in each.
        let mut buckets = [0usize; 10];
        for k in 0..1000 {
            buckets[(sh.at(k) * 10 / n) as usize] += 1;
        }
        let (lo, hi) = (*buckets.iter().min().unwrap(), *buckets.iter().max().unwrap());
        assert!(lo > 50 && hi < 160, "prefix is not spread: {buckets:?}");
        // The natural order would have put all 1,000 in bucket 0.
        assert!(buckets[0] < 200, "the order tracks the natural one: {buckets:?}");
    }
    /// `legal()` is a SECOND implementation of the verdict `nth()` reaches on
    /// the way — a neighbourhood move builds subsets directly instead of
    /// through an index, so the two have to agree or the search would accept
    /// builds the sweep rejects (and vice versa).
    #[test]
    fn legal_agrees_with_what_the_sweep_accepts() {
        let f = fams(&[
            None,
            Some("bane"),
            Some("bane"),
            None,
            Some("serration"),
            Some("serration"),
            None,
            None,
        ]);
        let usable: Vec<usize> = (0..f.len()).collect();
        for req in [&[][..], &[0][..], &[1][..], &[1, 4][..]] {
            let s = SubsetSpace::new(&f, &usable, req, 2, 4);
            let mut accepted: std::collections::BTreeSet<Vec<usize>> = Default::default();
            let mut buf = Vec::new();
            for i in 0..s.len() {
                if s.nth(i, &mut buf) {
                    let mut v = buf.clone();
                    v.sort_unstable();
                    accepted.insert(v);
                }
            }
            for m in 0u32..(1 << f.len()) {
                let v: Vec<usize> = (0..f.len()).filter(|b| m >> b & 1 == 1).collect();
                assert_eq!(
                    s.legal(&v),
                    accepted.contains(&v),
                    "legal() and the sweep disagree on {v:?} (required {req:?})"
                );
            }
        }
    }
    /// SHARDING must PARTITION the space: N workers walking strides
    /// `w, w+N, w+2N, …` must together visit every index exactly once. If the
    /// strides overlapped, workers would pay twice for the same build; if they
    /// left a gap, the union would silently miss builds while N shards all
    /// reported themselves exhaustive.
    #[test]
    fn shards_partition_the_shuffled_order_exactly() {
        for n in [1u128, 7, 100, 1000] {
            for shards in [1u128, 2, 3, 8] {
                let sh = Shuffle::new(n, 0xBEEF);
                let mut seen = vec![0u32; n as usize];
                for shard in 0..shards {
                    let mut k = 0u128;
                    while shard + k * shards < n {
                        seen[sh.at(shard + k * shards) as usize] += 1;
                        k += 1;
                    }
                }
                assert!(
                    seen.iter().all(|&c| c == 1),
                    "n={n} shards={shards}: {} indices missed, {} visited twice",
                    seen.iter().filter(|&&c| c == 0).count(),
                    seen.iter().filter(|&&c| c > 1).count()
                );
            }
        }
    }
}
