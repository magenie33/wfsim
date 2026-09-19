use super::*;

/// Replay ONE engagement, sampled into frames.
///
/// Deliberately NOT part of [`Summary`]: a summary is `Copy` and is produced
/// by the optimizer thousands of times a second, which must not pay for a
/// trace it never looks at. The simulator asks for this separately, with the
/// median run's `rng_state`, and gets the same fight back frame by frame.
///
/// ```ignore
/// let s = monte_carlo(&params, 100, seed);
/// let rep = replay(&params, s.median_run.rng_state, REPLAY_FRAMES);
/// ```
pub fn replay(params: &FightParams, rng_state: u64, frames: usize) -> Replay {
    // WHO IS WORTH FOLLOWING, decided by a DRY RUN of the same engagement.
    //
    // The run is reproducible bit-for-bit from `rng_state`, so asking it who
    // took damage and then replaying it again is exact rather than an estimate
    // — and it is the only way to rank bodies before the frames are recorded.
    // Two runs of one engagement, against 400 series on the wire.
    let scout = run_once(params, &mut Rng::new(rng_state));
    let mut ranked: Vec<(usize, f64)> = (1..=params.others.len())
        .map(|i| (i, scout.spread.by_body().0[i]))
        .filter(|(_, d)| *d > 0.0)
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    // THE AIMED BODY IS ALWAYS FIRST and always followed, however little it
    // took: it is the one the rest of the report is about.
    let mut follow: Vec<usize> = vec![0];
    follow.extend(ranked.into_iter().take(REPLAY_TRACKED - 1).map(|(i, _)| i));
    replay_following(params, rng_state, frames, &follow)
}

/// [`replay`], FOLLOWING BODIES SOMEBODY ASKED FOR — the whole of on-demand
/// tracking, and the reason a cap costs a reader nothing but a wait.
///
/// EXACT, NOT AN ESTIMATE. The run is reproducible bit-for-bit from
/// `rng_state`, so a body followed on the second asking gets the series it
/// would have had on the first. That is the same property the scout run above
/// already leans on, read the other way round: if you can re-run a fight to
/// RANK its bodies, you can re-run it to RECORD one.
///
/// CHEAPER THAN THE DEFAULT, because there is nothing to rank: one engagement
/// rather than the scout plus one. The aimed body is prepended whatever the
/// caller asked for — every series is read against it — and the list is
/// deduplicated and capped, since the wire's budget is a property of the
/// REPLAY and not of who is asking.
pub fn replay_following(
    params: &FightParams,
    rng_state: u64,
    frames: usize,
    want: &[usize],
) -> Replay {
    let mut follow: Vec<usize> = vec![0];
    for &i in want {
        if i != 0 && i <= params.others.len() && !follow.contains(&i) {
            follow.push(i);
        }
    }
    follow.truncate(REPLAY_TRACKED);

    let frames = frames.max(1);
    let mut rep = Replay {
        tracked: follow
            .iter()
            .map(|&i| {
                if i == 0 {
                    params.target_id.clone()
                } else {
                    params.others[i - 1].id.clone()
                }
            })
            .collect(),
        follow,
        frame_seconds: (params.duration_seconds / frames as f64).max(1e-6),
        buffs: params.buff_roster(),
        frames: Vec::with_capacity(frames),
    };
    run_once_traced(
        params,
        &mut Rng::new(rng_state),
        Some(&mut rep),
        &mut crate::record::Record::off(),
    );
    rep
}

/// THE COMBAT RECORD of one engagement — see [`crate::record`].
///
/// The same run [`replay`] plays back, asked a different question: not "what
/// did the curves look like" but "what happened, in order, and why was each
/// number the size it was". It re-runs from `rng_state`, so the record and the
/// replay are the same fight rather than two fights that agree on their totals.
///
/// A SEPARATE ENTRY POINT because the cost is the reader's to spend. The whole
/// stream of an ordinary fight is a few thousand events and can be handed over
/// entire; the densest build measured deals 408,817 damage instances in 180 s, which is why the window and the cap are arguments rather than
/// constants.
pub fn record(
    params: &FightParams,
    rng_state: u64,
    from: f64,
    to: f64,
    limit: usize,
    skip: usize,
) -> crate::record::Record {
    let mut rec = crate::record::Record::window(from, to, limit, skip);
    // WHAT THE ROWS' STACK LISTS ARE CALLED — the buff cards' own ids, because
    // they come from one place.
    rec.set_buffs(params.buff_roster().into_iter().map(|b| b.id).collect());
    run_once_traced(params, &mut Rng::new(rng_state), None, &mut rec);
    rec
}
