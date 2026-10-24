// SPDX-License-Identifier: AGPL-3.0-or-later
//! Work spread over the cores, for the passes that cross the whole library.

/// PAST THIS MANY THREADS THE DOOR GETS SLOWER, not faster: it allocates in
/// every step, and on 28 cores deriving the whole library took 70 s against
/// 21 s on eight.
const MAX_THREADS: usize = 8;

/// `f` over `items` on up to [`MAX_THREADS`] cores, in `items` order.
///
/// EACH THREAD TAKES THE NEXT ITEM, not a slice: a record's cost differs by
/// orders of magnitude (a riven of two elements orders its mods by search) and
/// a weapon's by its library, so equal slices leave one thread with the work.
pub(crate) fn par_map<T: Sync, U: Send>(items: &[T], f: impl Fn(&T) -> U + Sync) -> Vec<U> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let n = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_THREADS)
        .min(items.len().max(1));
    let mut out: Vec<(usize, U)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break };
                        mine.push((i, f(item)));
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a worker")).collect()
    });
    out.sort_unstable_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, u)| u).collect()
}
