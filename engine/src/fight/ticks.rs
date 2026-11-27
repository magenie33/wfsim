use crate::rules::math::Portable;
use super::*;

/// Timed STATUS events due strictly before `until`, in chronological order:
/// DoT ticks (Bleed/Toxin/Electricity/Gas + break-proc Tesla), the Heat
/// singleton's anchored ticks, and Blast fuse expiries. Mitigation is evaluated
/// LIVE at each event (the snapshot boundary rule); status damage never procs
/// status.
///
/// Lingering-FIELD ticks are NOT here — they are weapon damage that rolls its
/// own crit and its own status, so they get their own pass
/// ([`process_entities`]).
/// `rng` is the STATUS stream, and it is here for exactly one payload: a Blast
/// detonation triggers an EXTRA HIT, which rolls a status of its own. Every
/// other event this function settles is a payload already decided.
/// WHERE THE QUEUE STARTS PAYING, in live DoTs on one body.
///
/// Not tuned to a curve: it is a floor well above what a formation body carries
/// (a handful) and well below what a status build puts on the aimed one (291
/// measured). Anywhere between those two the choice does not matter, which is
/// what makes a single constant honest here rather than a fitted one.
pub(super) const TICK_QUEUE_MIN: usize = 32;

/// ELECTRICITY AND GAS TICK AS ONE GROUP — every live stack of the type pays
/// into one instance a second (wiki `Damage/Electricity Damage`, U33.6).
#[inline]
#[wfsim_gas_macro::metered]
pub(super) fn consolidated(t: DamageType) -> bool {
    matches!(t, DamageType::Electricity | DamageType::Gas)
}


/// ONE SCHEDULED STATUS EVENT, ordered the way the scan that preceded it was.
///
/// The scan took the strictly earliest event, considering DoTs in index order,
/// then Heat, then Blast — so a tie went to the first thing considered. `(t,
/// class, index)` ascending is that rule as a key, which is what lets a queue
/// replace the scan without moving a single number: an optimisation that
/// changes an answer is a bug, and the ORDER of these events is the order the
/// RNG is consumed in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TickKey {
    pub(super) t: f64,
    /// 0 = DoT, 1 = Heat, 2 = Blast — the order the scan considered them in.
    pub(super) class: u8,
    pub(super) index: u32,
}

#[wfsim_gas_macro::metered]
impl Eq for TickKey {}
#[wfsim_gas_macro::metered]
impl Ord for TickKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // `total_cmp` because these are clock readings: never NaN, and an
        // ordering that is total is what `BinaryHeap` requires.
        self.t
            .total_cmp(&other.t)
            .then(self.class.cmp(&other.class))
            .then(self.index.cmp(&other.index))
    }
}
#[wfsim_gas_macro::metered]
impl PartialOrd for TickKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const DOT_FAMILY_MASK: u32 = (1 << DamageType::Slash as u32)
    | (1 << DamageType::Electricity as u32)
    | (1 << DamageType::Toxin as u32)
    | (1 << DamageType::Gas as u32);

#[wfsim_gas_macro::metered]
impl DebuffState {
    #[inline]
    pub(super) fn has_fast_dot_schedule(&self) -> bool {
        self.live_dot_types != u32::MAX
            && (self.live_dot_types & !DOT_FAMILY_MASK) == 0
            && (self.live_dot_counts[0] == 0 || self.dot_rings[0].exact)
            && (self.live_dot_counts[1] == 0
                || self.consolidated_clocks[0].shared
                || self.dot_rings[1].exact
                || (!self.consolidated_clocks[0].phases_invalid && self.consolidated_clocks[0].phases_len > 1))
            && (self.live_dot_counts[2] == 0 || self.dot_rings[2].exact)
            && (self.live_dot_counts[3] == 0
                || self.consolidated_clocks[1].shared
                || self.dot_rings[3].exact
                || (!self.consolidated_clocks[1].phases_invalid && self.consolidated_clocks[1].phases_len > 1))
    }

    #[inline]
    fn first_live_consolidated_in_slice(&self, c: usize, dtype: DamageType) -> Option<(usize, &Dot)> {
        let h = self.consolidated_clocks[c].head as usize;
        if let Some(d) = self.dots.get(h) {
            if d.dtype == dtype && d.ticks_left > 0 {
                return Some((h, d));
            }
        }
        self.scan_first_live_consolidated_in_slice(c, dtype)
    }

    #[cold]
    fn scan_first_live_consolidated_in_slice(&self, c: usize, dtype: DamageType) -> Option<(usize, &Dot)> {
        let f = if c == 0 { 1 } else { 3 };
        if self.dot_rings[f].exact {
            let ring = &self.dot_rings[f];
            for k in 0..ring.len as usize {
                let idx = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                if let Some(d) = self.dots.get(idx) {
                    if d.dtype == dtype && d.ticks_left > 0 {
                        return Some((idx, d));
                    }
                }
            }
            return None;
        }
        let start = (self.consolidated_clocks[c].head as usize).min(self.dots.len());
        for (off, d) in self.dots[start..].iter().enumerate() {
            if d.dtype == dtype && d.ticks_left > 0 {
                return Some((start + off, d));
            }
        }
        None
    }

    #[cold]
    fn scan_phase_live_slow(&self, dtype: DamageType, phase: &debuffs::ClockPhase) -> Option<(u32, f64)> {
        let start = (phase.head as usize).min(self.dots.len());
        let end_excl = ((phase.tail as usize) + 1).min(self.dots.len());
        let bits = phase.next_tick.to_bits();
        for (off, d) in self.dots[start..end_excl].iter().enumerate() {
            if d.dtype == dtype && d.ticks_left > 0 && d.next_tick.to_bits() == bits {
                return Some(((start + off) as u32, d.next_tick));
            }
        }
        None
    }

    #[inline]
    pub(super) fn fast_dots_due(&self) -> f64 {
        let mut due = f64::INFINITY;
        for (c, f, dtype) in [(0, 1, DamageType::Electricity), (1, 3, DamageType::Gas)] {
            if self.live_dot_counts[f] > 0 {
                if self.consolidated_clocks[c].shared {
                    if let Some((_, d)) = self.first_live_consolidated_in_slice(c, dtype) {
                        due = due.min(d.next_tick);
                    }
                } else if self.dot_rings[f].exact {
                    let ring = &self.dot_rings[f];
                    for k in 0..ring.len as usize {
                        let idx = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                        if let Some(d) = self.dots.get(idx) {
                            if d.ticks_left > 0 {
                                due = due.min(d.next_tick);
                            }
                        }
                    }
                } else {
                    let slot = &self.consolidated_clocks[c];
                    for phase in &slot.phases[..slot.phases_len as usize] {
                        let h = phase.head as usize;
                        let nt = match self.dots.get(h) {
                            Some(d) if d.dtype == dtype && d.ticks_left > 0 && d.next_tick.to_bits() == phase.next_tick.to_bits() => {
                                Some(d.next_tick)
                            }
                            _ => self.scan_phase_live_slow(dtype, phase).map(|(_, t)| t),
                        };
                        if let Some(t) = nt {
                            due = due.min(t);
                        }
                    }
                }
            }
        }
        for f in [0, 2] {
            if self.live_dot_counts[f] > 0 {
                let ring = &self.dot_rings[f];
                for k in 0..ring.len as usize {
                    let idx = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                    if let Some(d) = self.dots.get(idx) {
                        if d.ticks_left > 0 {
                            due = due.min(d.next_tick);
                        }
                    }
                }
            }
        }
        due
    }

    /// THE SHAPE OF THE SCHEDULE, as three numbers.
    ///
    /// A queued event names its slot by INDEX, so anything that reshapes the
    /// collections invalidates the queue. Generation counters distinguish a
    /// Blast fuse pop (`blast.len()`) from DoT or Heat mutations during callbacks.
    #[inline]
    pub(super) fn tick_shape(&self) -> (u32, u32, usize) {
        (self.dots_gen, self.heat_gen, self.blast.len())
    }

    /// THE NEXT EVENT, BY SCANNING. The original rule, kept as the small-state
    /// path — and kept as the DEFINITION of the order, which the queue's key
    /// reproduces rather than replaces.
    pub(super) fn scan_next(&self, until: f64) -> Option<TickKey> {
        let mut best: Option<TickKey> = None;
        if !matches!(self.dots_due, Some(due) if due >= until) {
            if self.has_fast_dot_schedule() {
                for (c, f, dtype) in [(0, 1, DamageType::Electricity), (1, 3, DamageType::Gas)] {
                    if self.live_dot_counts[f] > 0 {
                        if self.consolidated_clocks[c].shared {
                            if let Some((i, d)) = self.first_live_consolidated_in_slice(c, dtype) {
                                if d.next_tick < until {
                                    let k = TickKey { t: d.next_tick, class: 0, index: i as u32 };
                                    if match best { Some(b) => k < b, None => true } {
                                        best = Some(k);
                                    }
                                }
                            }
                        } else if self.dot_rings[f].exact {
                            let ring = &self.dot_rings[f];
                            for k_idx in 0..ring.len as usize {
                                let i = ring.slots[(ring.head as usize + k_idx) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                                if let Some(d) = self.dots.get(i) {
                                    if d.ticks_left > 0 && d.next_tick < until {
                                        let k = TickKey { t: d.next_tick, class: 0, index: i as u32 };
                                        if match best { Some(b) => k < b, None => true } {
                                            best = Some(k);
                                        }
                                    }
                                }
                            }
                        } else {
                            let slot = &self.consolidated_clocks[c];
                            for phase in &slot.phases[..slot.phases_len as usize] {
                                let h = phase.head as usize;
                                let resolved = match self.dots.get(h) {
                                    Some(d) if d.dtype == dtype && d.ticks_left > 0 && d.next_tick.to_bits() == phase.next_tick.to_bits() => {
                                        Some((phase.head, d.next_tick))
                                    }
                                    _ => self.scan_phase_live_slow(dtype, phase),
                                };
                                if let Some((head, nt)) = resolved {
                                    if nt < until {
                                        let k = TickKey { t: nt, class: 0, index: head };
                                        if match best { Some(b) => k < b, None => true } {
                                            best = Some(k);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                for f in [0, 2] {
                    if self.live_dot_counts[f] > 0 {
                        let ring = &self.dot_rings[f];
                        for k_idx in 0..ring.len as usize {
                            let i = ring.slots[(ring.head as usize + k_idx) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                            if let Some(d) = self.dots.get(i) {
                                if d.ticks_left > 0 && d.next_tick < until {
                                    let k = TickKey { t: d.next_tick, class: 0, index: i as u32 };
                                    if match best { Some(b) => k < b, None => true } {
                                        best = Some(k);
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                for (i, d) in self.dots.iter().enumerate() {
                    if d.ticks_left > 0 && d.next_tick < until {
                        let k = TickKey { t: d.next_tick, class: 0, index: i as u32 };
                        if match best { Some(b) => k < b, None => true } {
                            best = Some(k);
                        }
                    }
                }
            }
        }
        if let Some(h) = &self.heat {
            if h.next_tick <= h.expiry && h.next_tick < until {
                let k = TickKey { t: h.next_tick, class: 1, index: 0 };
                if match best { Some(b) => k < b, None => true } {
                    best = Some(k);
                }
            }
        }
        for (i, b) in self.blast.iter().enumerate() {
            if b.fuse < until {
                let k = TickKey { t: b.fuse, class: 2, index: i as u32 };
                if match best { Some(b) => k < b, None => true } {
                    best = Some(k);
                }
            }
        }
        best
    }

    /// Every event this state has pending before `until`, into a queue the
    /// caller owns — see [`Self::tick_q`] for why it is not returned.
    /// Returns a lower bound on the earliest unqueued live DoT `next_tick`.
    pub(super) fn refill_tick_queue(
        &self,
        q: &mut std::collections::BinaryHeap<std::cmp::Reverse<TickKey>>,
        until: f64,
    ) -> f64 {
        q.clear();
        let mut unqueued_due = f64::INFINITY;
        // A CONSOLIDATED GROUP IS ONE EVENT, so it is queued once: the stack
        // with the lowest index, which is the key that fires today — every
        // other stack's key only ever came off the queue stale.
        if matches!(self.dots_due, Some(due) if due >= until) {
            unqueued_due = self.dots_due.unwrap_or(f64::INFINITY);
        } else if self.live_dot_types != u32::MAX
            && (self.live_dot_types & !DOT_FAMILY_MASK) == 0
            && (self.live_dot_counts[0] == 0 || self.dot_rings[0].exact)
            && (self.live_dot_counts[1] == 0 || self.consolidated_clocks[0].shared)
            && (self.live_dot_counts[2] == 0 || self.dot_rings[2].exact)
            && (self.live_dot_counts[3] == 0 || self.consolidated_clocks[1].shared)
        {
            for (c, f, dtype) in [(0, 1, DamageType::Electricity), (1, 3, DamageType::Gas)] {
                if self.live_dot_counts[f] > 0 {
                    if let Some((i, d)) = self.first_live_consolidated_in_slice(c, dtype) {
                        if d.next_tick < until {
                            q.push(std::cmp::Reverse(TickKey { t: d.next_tick, class: 0, index: i as u32 }));
                        } else if d.next_tick < unqueued_due {
                            unqueued_due = d.next_tick;
                        }
                    }
                }
            }
            for f in [0, 2] {
                if self.live_dot_counts[f] > 0 {
                    let ring = &self.dot_rings[f];
                    for k in 0..ring.len as usize {
                        let i = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                        if let Some(d) = self.dots.get(i) {
                            if d.ticks_left > 0 {
                                if d.next_tick < until {
                                    q.push(std::cmp::Reverse(TickKey { t: d.next_tick, class: 0, index: i as u32 }));
                                } else if d.next_tick < unqueued_due {
                                    unqueued_due = d.next_tick;
                                }
                            }
                        }
                    }
                }
            }
        } else if self.has_fast_dot_schedule() {
            let mut lists = [[0u32; debuffs::DOT_CAP_RING_MAX]; 2];
            let mut lens = [0usize; 2];
            for (c, f, dtype) in [(0, 1, DamageType::Electricity), (1, 3, DamageType::Gas)] {
                if self.live_dot_counts[f] == 0 {
                    continue;
                }
                if self.consolidated_clocks[c].shared {
                    if let Some((i, _)) = self.first_live_consolidated_in_slice(c, dtype) {
                        lists[c][0] = i as u32;
                        lens[c] = 1;
                    }
                } else if self.dot_rings[f].exact {
                    let ring = &self.dot_rings[f];
                    let mut out = 0usize;
                    for k in 0..ring.len as usize {
                        let idx = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)];
                        if let Some(d) = self.dots.get(idx as usize) {
                            if d.ticks_left > 0 {
                                lists[c][out] = idx;
                                out += 1;
                            }
                        }
                    }
                    lens[c] = out;
                } else {
                    let slot = &self.consolidated_clocks[c];
                    let mut out = 0usize;
                    for phase in &slot.phases[..slot.phases_len as usize] {
                        let h = phase.head as usize;
                        let head_opt = match self.dots.get(h) {
                            Some(d) if d.dtype == dtype && d.ticks_left > 0 && d.next_tick.to_bits() == phase.next_tick.to_bits() => {
                                Some(phase.head)
                            }
                            _ => self.scan_phase_live_slow(dtype, phase).map(|(idx, _)| idx),
                        };
                        if let Some(head) = head_opt {
                            let mut ins = out;
                            while ins > 0 && lists[c][ins - 1] > head {
                                lists[c][ins] = lists[c][ins - 1];
                                ins -= 1;
                            }
                            lists[c][ins] = head;
                            out += 1;
                        }
                    }
                    lens[c] = out;
                }
            }
            let mut groups = [(DamageType::Impact, 0.0f64, None); 8];
            let mut n = 0usize;
            let mut p0 = 0usize;
            let mut p1 = 0usize;
            let mut overflowed_phases = false;
            while p0 < lens[0] || p1 < lens[1] {
                let idx = if p1 >= lens[1] || (p0 < lens[0] && lists[0][p0] <= lists[1][p1]) {
                    let v = lists[0][p0];
                    p0 += 1;
                    v
                } else {
                    let v = lists[1][p1];
                    p1 += 1;
                    v
                };
                let d = &self.dots[idx as usize];
                let nt = d.next_tick;
                if nt >= until {
                    if nt < unqueued_due {
                        unqueued_due = nt;
                    }
                    if n == groups.len() {
                        continue;
                    }
                }
                let mut matched_at = None;
                for g in &groups[..n] {
                    if g.0 == d.dtype && g.2 == d.native_group && (g.1 - nt).abs() < 1e-9 {
                        matched_at = Some(g.1);
                        break;
                    }
                }
                if let Some(at) = matched_at {
                    if at >= until && nt < unqueued_due {
                        unqueued_due = nt;
                    }
                    continue;
                }
                if n < groups.len() {
                    groups[n] = (d.dtype, nt, d.native_group);
                    n += 1;
                } else if nt < until && (!self.dot_rings[1].exact || !self.dot_rings[3].exact) {
                    overflowed_phases = true;
                    break;
                }
                if nt < until {
                    q.push(std::cmp::Reverse(TickKey { t: nt, class: 0, index: idx }));
                }
            }
            if overflowed_phases {
                return self.refill_tick_queue_slow(q, until);
            }
            for f in [0, 2] {
                if self.live_dot_counts[f] > 0 {
                    let ring = &self.dot_rings[f];
                    for k in 0..ring.len as usize {
                        let i = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                        if let Some(d) = self.dots.get(i) {
                            if d.ticks_left > 0 {
                                if d.next_tick < until {
                                    q.push(std::cmp::Reverse(TickKey { t: d.next_tick, class: 0, index: i as u32 }));
                                } else if d.next_tick < unqueued_due {
                                    unqueued_due = d.next_tick;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            return self.refill_tick_queue_slow(q, until);
        }
        if let Some(h) = &self.heat {
            if h.next_tick <= h.expiry && h.next_tick < until {
                q.push(std::cmp::Reverse(TickKey { t: h.next_tick, class: 1, index: 0 }));
            }
        }
        for (i, b) in self.blast.iter().enumerate() {
            if b.fuse < until {
                q.push(std::cmp::Reverse(TickKey { t: b.fuse, class: 2, index: i as u32 }));
            }
        }
        unqueued_due
    }

    #[cold]
    fn refill_tick_queue_slow(
        &self,
        q: &mut std::collections::BinaryHeap<std::cmp::Reverse<TickKey>>,
        until: f64,
    ) -> f64 {
        q.clear();
        let mut unqueued_due = f64::INFINITY;
        let mut groups = [(DamageType::Impact, 0.0f64, None); 8];
        let mut n = 0usize;
        let mut last_hit: Option<(DamageType, u64, Option<std::num::NonZeroU16>)> = None;
        for (i, d) in self.dots.iter().enumerate() {
            if d.ticks_left == 0 {
                continue;
            }
            let nt = d.next_tick;
            if nt >= until {
                if nt < unqueued_due {
                    unqueued_due = nt;
                }
                if n == groups.len() {
                    continue;
                }
            }
            if consolidated(d.dtype) {
                let bits = nt.to_bits();
                if last_hit == Some((d.dtype, bits, d.native_group)) {
                    continue;
                }
                let mut matched_at = None;
                for g in &groups[..n] {
                    if g.0 == d.dtype && g.2 == d.native_group && (g.1 - nt).abs() < 1e-9 {
                        matched_at = Some(g.1);
                        break;
                    }
                }
                if let Some(at) = matched_at {
                    if nt.is_finite() {
                        last_hit = Some((d.dtype, bits, d.native_group));
                    }
                    if at >= until && nt < unqueued_due {
                        unqueued_due = nt;
                    }
                    continue;
                }
                if n < groups.len() {
                    groups[n] = (d.dtype, nt, d.native_group);
                    n += 1;
                    if nt.is_finite() {
                        last_hit = Some((d.dtype, bits, d.native_group));
                    }
                }
            }
            if nt < until {
                q.push(std::cmp::Reverse(TickKey { t: nt, class: 0, index: i as u32 }));
            }
        }
        if let Some(h) = &self.heat {
            if h.next_tick <= h.expiry && h.next_tick < until {
                q.push(std::cmp::Reverse(TickKey { t: h.next_tick, class: 1, index: 0 }));
            }
        }
        for (i, b) in self.blast.iter().enumerate() {
            if b.fuse < until {
                q.push(std::cmp::Reverse(TickKey { t: b.fuse, class: 2, index: i as u32 }));
            }
        }
        unqueued_due
    }
}

/// EVERY BODY BUT THE AIMED ONE, drained to `until`.
///
/// The aimed body's clock is the run loop's own and is settled with the rest of
/// it; these are the neighbours a chain hop, a splash, a tendril or an echo
/// reached. The shot path and the END of the engagement share this because they
/// share the DECISION — a status on a neighbour is paid out, or it was never
/// applied at all — and the tail drained only the aimed body, so whatever was
/// still burning on a neighbour when firing stopped was recorded and never paid.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn settle_crowd_ticks(
    w: &mut CardWindows,
    bodies: &mut [Body],
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
) {
    for (b, foe) in bodies.iter_mut().enumerate().skip(1) {
        // NOTHING TO BURN, NOTHING TO DO. A formation is up to 400 bodies
        // and a shot reaches a handful; walking the rest once per shot is
        // the whole difference between a crowd being affordable and not.
        if foe.debuffs.idle() {
            continue;
        }
        let Some(spec) = params.body(b) else { continue };
        process_ticks(w, foe, gal, arc, until, params, active, r, rec, rng, spec.params, b);
    }
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn process_ticks(
    // THE SHOOTER'S LIVE WINDOWS, because a burning cloud reads them: an
    // element buff strengthens what is already burning and stops the moment it
    // lapses (`FightParams::element_at`). A tick loop that could not see them
    // would have to freeze the answer at the proc, which is a different number.
    w: &mut CardWindows,
    // THE BODY THIS LANDS ON. Its pools and the statuses on them are
    // one thing, so they arrive as one.
    body: &mut Body,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    // WHICH BODY'S ticks these are — see `settle_procs`'s parameter of the
    // same name.
    foe: &Foe,
    // WHICH BODY IS TICKING, in the fight's numbering — `FightParams::body`.
    //
    // A FORMATION BODY NEVER TICKED AT ALL until 2026-08-17: this was called
    // for the aimed body and for nothing else, so every status a chain hop, a
    // splash, a tendril or an echo applied to a neighbour was recorded and
    // never paid out. Gas and Electricity are the two elements whose PROC is an
    // area, and they cannot work at all without it.
    body_index: usize,
) {
    let Body { state: target, debuffs, .. } = body;
    enum Ev {
        Dot(usize),
        Heat,
        Blast(usize),
    }
    let p = foe;
    let status_damage = params.status_duration_multiplier;
    // A QUEUE, NOT A SCAN, above the threshold. Asking every pending status on
    // a body which of them is next, once per EVENT, measures at 291 live DoTs,
    // 58,918 tick events and 18.2 MILLION scan iterations a run on a dense
    // status build — 80% of the engagement in this function. The 291 is not a
    // leak: an ordinary enemy has no stack cap, so `dot_cap` is `None` on
    // purpose and the list is that long because the game's is.
    //
    // …AND ONLY WHERE IT PAYS. A queue costs one pass plus `log n` an event; a
    // scan costs `n` an event and nothing to set up, so on a body with three
    // DoTs the scan wins outright — and a formation is mostly those, ~300,000
    // per-body calls a run on a 19x19. Building a heap for each measured 444
    // multishot a run against 831.
    //
    // The shape of the state picks the path, and the two are one loop:
    // `scan_next` is the ORDER's definition and `TickKey` reproduces it, which
    // keeps the answer identical on either side of the threshold. An empty
    // `BinaryHeap` allocates nothing.
    // WHICH SHOT THE EVENT BEING SETTLED BELONGS TO — set by each arm below.
    let mut seeded_by;
    // WHOSE STACK IS PAYING OUT. A pile is the BODY's and its ticks land long
    // after the shot, so without this the damage goes to whoever happens to be
    // firing when it ticks. Heat and Blast consolidate every stack into one
    // event and cannot name one applier, so they answer the wielder and say so
    // where they set it.
    let mut dot_owner;
    // …AND THE TWO HALVES OF A DoT TICK, for the ledger alone: the seeds it
    // holds and what the accumulator's own 1 is worth. `None` when nothing
    // records. Kept here rather than derived at the ledger
    // because only this loop can still see them apart — see `Dot::live` and
    // `Dot::accumulator_unit`, which take DIFFERENT faction layers.
    let mut dot_parts: Option<Vec<crate::record::Part>>;
    // NOTHING DUE, NOTHING TO DO: the loop below would pop no event, so an
    // answer from the bound and a live look at Heat and Blast is the same one
    // without rebuilding a queue over every DoT on the body.
    if matches!(debuffs.dots_due, Some(due) if due >= until)
        && !matches!(&debuffs.heat, Some(h) if h.next_tick <= h.expiry && h.next_tick < until)
    {
        let mut blast_due = false;
        for b in &debuffs.blast {
            if b.fuse < until {
                blast_due = true;
                break;
            }
        }
        if !blast_due {
            return;
        }
    }
    let use_queue = debuffs.active_dot_count() > TICK_QUEUE_MIN;
    let mut q = std::mem::take(&mut debuffs.tick_q);
    let mut unqueued_due = if use_queue {
        debuffs.refill_tick_queue(&mut q, until)
    } else {
        f64::INFINITY
    };
    let mut advanced_due = f64::INFINITY;
    let mut any_dot_expired = false;
    let mut can_skip_retain = use_queue;
    let initial_dots_gen = debuffs.dots_gen;
    let mut shape = debuffs.tick_shape();
    loop {
        let k = if use_queue {
            let Some(std::cmp::Reverse(k)) = q.pop() else { break };
            k
        } else {
            let Some(k) = debuffs.scan_next(until) else { break };
            k
        };
        // A STALE KEY. A consolidated family advances EVERY one of its live
        // instances when the first of them fires, so the queue still holds
        // entries for ticks that have already been paid. The scan path never
        // produces one — it reads `next_tick` live — which is why this guard is
        // here rather than inside `refill_tick_queue`: both paths must answer
        // identically, and only one of them can go stale.
        if k.class == 0 {
            match debuffs.dots.get(k.index as usize) {
                Some(d) if d.ticks_left > 0 && (d.next_tick - k.t).abs() < 1e-9 => {}
                _ => continue,
            }
        }
        let (now, ev) = match k.class {
            0 => (k.t, Ev::Dot(k.index as usize)),
            1 => (k.t, Ev::Heat),
            _ => (k.t, Ev::Blast(k.index as usize)),
        };

        let mit = debuffs.mitigation(now, status_damage, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
        // A tick is one damage type — which is also the type the
        // vulnerability column reads. Bleed is the exception: it is stored
        // under Slash (that is the proc that made it) but the damage is
        // CINEMATIC, which takes no faction modifier anywhere, and
        // `ignores_armor` is this file's marker for it.
        // `xh` is the EXTRA HIT bracket, and `Some` is what says this payload
        // triggers one at all. A DoT tick and a Heat tick are `None` — no extra
        // hit fires off a status — and the Blast detonation is the documented
        // exception (wiki `Extra_Hit`, Bugs: "Only Xata's Whisper will be
        // triggered by blast Detonations, no other extra hit will").
        let mut tick_landing = ProcLanding::default();
        let mut native_layers = None;
        let (value, ignores_armor, is_dot_tick, hit_type, src, xh) = match &ev {
            Ev::Dot(i) => {
                let dtype = debuffs.dots[*i].dtype;
                let ignores_armor = debuffs.dots[*i].ignores_armor;
                tick_landing = debuffs.proc_landing(&debuffs.dots[*i]);
                // THE ROW POINTS BACK AT THE ROUND THAT SEEDED IT, not at
                // whatever the weapon is doing four seconds later. Swapped for
                // the duration of this settlement and put back below, so the
                // recorder's "current shot" stays the loop's business.
                seeded_by = debuffs.dots[*i].cause;
                dot_owner = debuffs.dots[*i].owner;
                let group_id = debuffs.dots[*i].native_group;
                let c_slot = debuffs::consolidated_slot(dtype);
                let family_shared_clock = c_slot.is_some_and(|c| {
                    debuffs.live_dot_types != u32::MAX && debuffs.consolidated_clocks[c].shared
                }) && debuffs.dots[*i].ticks_left > 0;
                let exact_phase = if !family_shared_clock {
                    c_slot.and_then(|c| {
                        let f = if c == 0 { 1 } else { 3 };
                        if debuffs.live_dot_types != u32::MAX
                            && !debuffs.dot_rings[f].exact
                            && debuffs.dots[*i].ticks_left > 0
                        {
                            debuffs.consolidated_clocks[c].active_phase_for(now, *i)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
                let phase_shared_clock = family_shared_clock || exact_phase.is_some();
                let (start, end_excl) = if let Some((_, p_start, p_end_incl)) = exact_phase {
                    (p_start, (p_end_incl + 1).min(debuffs.dots.len()))
                } else if family_shared_clock {
                    (*i, debuffs.dots.len())
                } else {
                    (
                        c_slot.map_or(0, |c| (debuffs.consolidated_clocks[c].head as usize).min(*i)),
                        debuffs.dots.len(),
                    )
                };
                let is_consolidated = consolidated(dtype);
                let native_radial = if !is_consolidated {
                    false
                } else if debuffs.has_only_native_radial_seeds(dtype) {
                    true
                } else {
                    let mut all_native = true;
                    for d in &debuffs.dots[start..end_excl] {
                        if d.ticks_left > 0
                            && d.dtype == dtype
                            && (family_shared_clock || (d.native_group == group_id && (d.next_tick - now).abs() < 1e-9))
                            && !d.has_native_radial_seed()
                        {
                            all_native = false;
                            break;
                        }
                    }
                    all_native
                };
                let mut expired_in_event = 0u32;
                let mut first_surviving_idx: Option<u32> = None;
                let mut last_surviving_idx = 0u32;
                let value = if native_radial {
                    use crate::rules::status::radial::{RadialGroup, RecordContext};
                    let source = crate::fight::dot::Source::at(params, dtype, now, w);
                    let f1 = source.faction;
                    let f2 = f1 * f1;
                    let mut group = RadialGroup::default();
                    let mut last_bracket = None;
                    let mut inputs = Vec::new();
                    let trace_native = rec.native_trace_is_on();
                    let recording = rec.is_on();
                    let group_key = trace_native.then(|| debuffs.group_key(group_id)).flatten();
                    if !trace_native && !recording {
                        let mut last_idx = None;
                        let ring_opt = debuffs::dot_family_slot(dtype)
                            .and_then(|f| debuffs.dot_rings[f].exact.then_some(debuffs.dot_rings[f]));
                        if let Some(ring) = ring_opt {
                            for k in 0..ring.len as usize {
                                let idx = ring.slots[(ring.head as usize + k) & (debuffs::DOT_CAP_RING_MAX - 1)] as usize;
                                if idx < start || idx >= end_excl {
                                    continue;
                                }
                                let d = &mut debuffs.dots[idx];
                                if d.ticks_left == 0
                                    || d.dtype != dtype
                                    || (!family_shared_clock
                                        && (d.native_group != group_id || (d.next_tick - now).abs() >= 1e-9))
                                {
                                    continue;
                                }
                                d.next_tick += 1.0;
                                d.ticks_left -= 1;
                                if d.ticks_left == 0 {
                                    any_dot_expired = true;
                                    expired_in_event += 1;
                                } else {
                                    if first_surviving_idx.is_none() {
                                        first_surviving_idx = Some(idx as u32);
                                    }
                                    last_surviving_idx = idx as u32;
                                    if !phase_shared_clock && d.next_tick < advanced_due {
                                        advanced_due = d.next_tick;
                                    }
                                }
                                let crate::model::StoredStatusSeed::Radial { retained_seed: seed, .. } = d.retained else {
                                    unreachable!("native radial record");
                                };
                                let f_mult = match d.depth.saturating_sub(1) {
                                    0 => 1.0,
                                    1 => f1,
                                    2 => f2,
                                    other => faction_at(f1, other),
                                };
                                group.accumulator += (seed as f64 * f_mult) as f32;
                                last_idx = Some(idx);
                            }
                        } else {
                            for (off, d) in debuffs.dots[start..end_excl].iter_mut().enumerate() {
                                if d.ticks_left == 0
                                    || d.dtype != dtype
                                    || (!family_shared_clock
                                        && (d.native_group != group_id || (d.next_tick - now).abs() >= 1e-9))
                                {
                                    continue;
                                }
                                d.next_tick += 1.0;
                                d.ticks_left -= 1;
                                if d.ticks_left == 0 {
                                    any_dot_expired = true;
                                    expired_in_event += 1;
                                } else {
                                    let idx32 = (start + off) as u32;
                                    if first_surviving_idx.is_none() {
                                        first_surviving_idx = Some(idx32);
                                    }
                                    last_surviving_idx = idx32;
                                    if !phase_shared_clock && d.next_tick < advanced_due {
                                        advanced_due = d.next_tick;
                                    }
                                }
                                let crate::model::StoredStatusSeed::Radial { retained_seed: seed, .. } = d.retained else {
                                    unreachable!("native radial record");
                                };
                                let f_mult = match d.depth.saturating_sub(1) {
                                    0 => 1.0,
                                    1 => f1,
                                    2 => f2,
                                    other => faction_at(f1, other),
                                };
                                group.accumulator += (seed as f64 * f_mult) as f32;
                                last_idx = Some(start + off);
                            }
                        }
                        if phase_shared_clock {
                            if let Some(surv) = first_surviving_idx {
                                let nt = debuffs.dots[surv as usize].next_tick;
                                if nt < advanced_due {
                                    advanced_due = nt;
                                }
                            }
                        }
                        if let Some(li) = last_idx {
                            let last = &debuffs.dots[li];
                            tick_landing = last.landing;
                            last_bracket = Some(last.bracket);
                            seeded_by = last.cause;
                            dot_owner = last.owner;
                        }
                    } else {
                        for (off, d) in debuffs.dots[start..end_excl].iter_mut().enumerate() {
                            if d.ticks_left == 0
                                || d.dtype != dtype
                                || (!family_shared_clock
                                    && (d.native_group != group_id || (d.next_tick - now).abs() >= 1e-9))
                            {
                                continue;
                            }
                            d.next_tick += 1.0;
                            d.ticks_left -= 1;
                            if d.ticks_left == 0 {
                                any_dot_expired = true;
                                expired_in_event += 1;
                            } else {
                                let idx32 = (start + off) as u32;
                                if first_surviving_idx.is_none() {
                                    first_surviving_idx = Some(idx32);
                                }
                                last_surviving_idx = idx32;
                                if d.next_tick < advanced_due {
                                    advanced_due = d.next_tick;
                                }
                            }
                            let crate::model::StoredStatusSeed::Radial { retained_seed: seed, .. } = d.retained else {
                                unreachable!("native radial record");
                            };
                            let f_mult = match d.depth.saturating_sub(1) {
                                0 => 1.0,
                                1 => f1,
                                2 => f2,
                                other => faction_at(f1, other),
                            };
                            let input = (seed as f64 * f_mult) as f32;
                            let part = d.native_part.unwrap_or(if d.landing.head { crate::model::HitPart::HEAD } else { crate::model::HitPart::BODY });
                            if trace_native {
                                let before = group.accumulator;
                                group.visit(input, RecordContext { part74: part.0, ..Default::default() });
                                rec.trace_native(now, || crate::record::NativeStep {
                                    phase: "status_tick", group: group_key, part: Some(group.part55),
                                    proc_type: crate::model::ProcType::from_damage_type(dtype).map(|p| p.id()),
                                    context_binding: if group_id.is_some() { "caller_supplied_group" } else { "scene_group" },
                                    ..crate::record::NativeStep::new(now, body_index, "ALN-PROC-03.accumulate",
                                        &[before, input], group.accumulator)
                                });
                            } else {
                                group.accumulator += input;
                                group.count = group.count.wrapping_add(1);
                                group.part55 = part.0;
                            }
                            if recording {
                                inputs.push(crate::record::Part { factor: crate::record::Factor::StatusSeeds,
                                    amount: input as f64, head: input as f64,
                                    head_factor: Some(crate::record::Factor::StatusSeeds), of: Vec::new() });
                            }
                            tick_landing = d.landing;
                            last_bracket = Some(d.bracket);
                            seeded_by = d.cause;
                            dot_owner = d.owner;
                        }
                    }
                    let own_element = last_bracket.map_or(0.0, |bracket| bracket + source.element - 1.0);
                    let preparation = group.prepare_steps(0.5, own_element as f32);
                    let prepared = preparation.damage;
                    if trace_native {
                        for (stage, inputs, output) in [
                            ("ALN-PROC-03.coefficient", [group.accumulator, 0.5], preparation.scaled),
                            ("ALN-PROC-03.minimum", [preparation.scaled, 1.0], preparation.clamped),
                            ("ALN-PROC-03.own_element", [preparation.clamped, preparation.element], prepared),
                        ] {
                            rec.trace_native(now, || crate::record::NativeStep {
                                phase: "status_tick", group: group_key, part: Some(group.part55),
                                proc_type: crate::model::ProcType::from_damage_type(dtype).map(|p| p.id()),
                                context_binding: if group_id.is_some() { "caller_supplied_group" } else { "scene_group" },
                                ..crate::record::NativeStep::new(now, body_index, stage, &inputs, output)
                            });
                        }
                    }
                    let value = prepared as f64 * source.faction * tick_landing.factor;
                    native_layers = rec.is_on().then(|| {
                        use crate::record::{Factor, Layer, Part};
                        inputs.insert(0, Part { factor: Factor::StatusAccumulator,
                            amount: 1.0, head: 1.0, head_factor: None, of: Vec::new() });
                        let rounding = group.accumulator as f64 - inputs.iter().map(|p| p.amount).sum::<f64>();
                        if rounding != 0.0 {
                            inputs.push(Part { factor: Factor::FloatRounding, amount: rounding,
                                head: rounding, head_factor: None, of: Vec::new() });
                        }
                        let half = preparation.scaled;
                        let mut layers = vec![Layer::Sum { factor: Factor::StatusSeeds,
                            parts: inputs, out: group.accumulator as f64 },
                            Layer::Mul { factor: Factor::StatusCoefficient, value: 0.5,
                                head: 1.0, of: Vec::new(), out: half as f64 }];
                        if half < 1.0 {
                            layers.push(Layer::Clamp { factor: Factor::DamageFloor, minimum: 1.0, out: 1.0 });
                        }
                        layers.push(Layer::Mul { factor: Factor::ElementBracket,
                            value: preparation.element as f64,
                            head: 1.0, of: Vec::new(), out: prepared as f64 });
                        layers.push(Layer::Mul { factor: Factor::Faction, value: source.faction,
                            head: 1.0, of: Vec::new(), out: prepared as f64 * source.faction });
                        layers.push(Layer::Mul { factor: Factor::TickLanding, value: tick_landing.factor,
                            head: 1.0, of: Vec::new(), out: value });
                        layers
                    });
                    dot_parts = None;
                    value
                } else if is_consolidated {
                    for d in &debuffs.dots[start..end_excl] {
                        if d.ticks_left > 0
                            && d.dtype == dtype
                            && (family_shared_clock || (d.native_group == group_id && (d.next_tick - now).abs() < 1e-9))
                        {
                            tick_landing = d.landing;
                            seeded_by = d.cause;
                            dot_owner = d.owner;
                        }
                    }
                    let mut sum = 0.0;
                    // ONE ACCUMULATOR FOR THE WHOLE GROUP, taken from whichever
                    // seed joined it first — "they are added to the same
                    // accumulator, so its initial value of 1 is included only
                    // once" (`Dot::accumulator_unit`). Adding it per stack is
                    // the exact mistake the page calls out.
                    let mut unit = 0.0;
                    // EACH STACK ITS OWN PART, since the stacks of one group
                    // need not share a seed, a depth or a landing.
                    let mut parts: Vec<crate::record::Part> = Vec::new();
                    let mut acc_part = None;
                    let source = crate::fight::dot::Source::at(params, dtype, now, w);
                    for (off, d) in debuffs.dots[start..end_excl].iter_mut().enumerate() {
                        if d.ticks_left > 0
                            && d.dtype == dtype
                            && (family_shared_clock || (d.native_group == group_id && (d.next_tick - now).abs() < 1e-9))
                        {
                            d.next_tick += 1.0;
                            d.ticks_left -= 1;
                            if d.ticks_left == 0 {
                                any_dot_expired = true;
                                expired_in_event += 1;
                            } else {
                                let idx32 = (start + off) as u32;
                                if first_surviving_idx.is_none() {
                                    first_surviving_idx = Some(idx32);
                                }
                                last_surviving_idx = idx32;
                                if d.next_tick < advanced_due {
                                    advanced_due = d.next_tick;
                                }
                            }
                            // AT `now`, NOT AT THE MOMENT IT WAS APPLIED — see
                            // `Dot::live`. A buff the shooter gained while this
                            // was burning is already in it.
                            // M100: the latest application changes the whole
                            // tick's landing without changing any hit seed.
                            let paid = d.live(&source) * tick_landing.factor;
                            sum += paid;
                            let first = unit == 0.0;
                            if first {
                                unit = d.accumulator_unit(&source) * tick_landing.factor;
                            }
                            if rec.is_on() {
                                let (over_seed, over_acc) = d.explain(params, now, w);
                                let landed = Dot { landing: tick_landing, ..*d };
                                parts.push(landed.seed_part(rec, paid, over_seed, true));
                                if first && unit != 0.0 {
                                    acc_part = Some(landed.accumulator_part(unit, over_acc, true));
                                }
                            }
                        }
                    }
                    dot_parts = rec.is_on().then(|| {
                        parts.extend(acc_part);
                        parts
                    });
                    sum + unit
                } else {
                    let d = &mut debuffs.dots[*i];
                    d.next_tick += 1.0;
                    d.ticks_left -= 1;
                    if d.ticks_left == 0 {
                        any_dot_expired = true;
                        expired_in_event = 1;
                    } else if d.next_tick < advanced_due {
                        advanced_due = d.next_tick;
                    }
                    // Slash and Toxin tick independently, so each stack is its
                    // own tick group and carries its own accumulator.
                    let source = crate::fight::dot::Source::at(params, d.dtype, now, w);
                    let (seeds, acc) = (d.live(&source), d.accumulator_unit(&source));
                    // THE LEDGER'S HALF, and only the ledger reads it: explaining a
                    // tick allocates, so a fight nobody records does not.
                    dot_parts = rec.is_on().then(|| {
                        let (over_seed, over_acc) = d.explain(params, now, w);
                        vec![
                            d.seed_part(rec, seeds, over_seed, false),
                            d.accumulator_part(acc, over_acc, false),
                        ]
                    });
                    seeds + acc
                };
                if expired_in_event > 0 {
                    debuffs.note_expired_dots(dtype, expired_in_event);
                }
                if let Some(c) = c_slot {
                    let f = if c == 0 { 1 } else { 3 };
                    if debuffs.live_dot_counts[f] > 0 {
                        if let Some((p_idx, _, _)) = exact_phase {
                            let surv = first_surviving_idx.map(|first| {
                                (first, last_surviving_idx, debuffs.dots[first as usize].next_tick)
                            });
                            let dots_len = debuffs.dots.len() as u32;
                            debuffs.consolidated_clocks[c].finish_phase_tick(p_idx, surv, dots_len);
                        } else if family_shared_clock {
                            if let Some(surv) = first_surviving_idx {
                                debuffs.consolidated_clocks[c].head = surv;
                                debuffs.consolidated_clocks[c].tail = last_surviving_idx;
                                debuffs.consolidated_clocks[c].next_tick = debuffs.dots[surv as usize].next_tick;
                            }
                        }
                    }
                }
                let hit_type = if ignores_armor {
                    DamageType::Cinematic
                } else {
                    dtype
                };
                (value, ignores_armor, true, hit_type, dtype, None)
            }
            Ev::Heat => {
                // A SINGLETON HAS NO ONE SEEDER: every proc refreshes the same
                // entity, so the tick belongs to all of them and to none. Left
                // unattributed rather than credited to whichever proc came
                // first, which would read as a fact and be an arbitrary pick.
                seeded_by = u32::MAX;
                // ONE EVENT FOR EVERY STACK, so there is no one applier to
                // name — the same reason `seeded_by` is unset here. Credited
                // to the wielder rather than to whoever fired last, which
                // would read as a fact and be an arbitrary pick.
                dot_owner = Seat::WIELDER;
                let h = debuffs.heat.as_mut().expect("heat event needs entity");
                h.next_tick += 1.0;
                // AT `now`. See `Dot::live` — the same rule, on the one status
                // whose stacks all share a clock.
                let bracket = h.bracket + params.element_at(DamageType::Heat, now, w);
                let f = params.faction_at_time(now);
                // ONE ACCUMULATOR for the whole consolidated tick, whatever
                // `stacks` says — `Dot::accumulator_unit`, and Heat is the case
                // the page spells out.
                let seeds = h.value * bracket * faction_at(f, h.depth);
                let acc = h.unit * bracket * f;
                // THE BURN AND ITS `1`, each over the shooter's bracket and the
                // target's multiplier at its own depth — `Dot::explain`'s split.
                dot_parts = rec.is_on().then(|| {
                    use crate::record::{Factor, Part, Scale};
                    let (fb, m) = (params.faction_bracket_at(now), params.foe.faction_bracket_multiplier);
                    let over = |depth: u32| crate::fight::dot::drop_ones(vec![
                        Scale { factor: Factor::ElementBracket, value: bracket },
                        Scale { factor: Factor::Faction, value: faction_at(fb, depth) },
                        Scale { factor: Factor::TargetMultiplier, value: faction_at(m, depth) },
                    ]);
                    vec![
                        Part { factor: Factor::StatusSeeds, amount: seeds, head: h.value,
                            head_factor: Some(Factor::HeatBurn), of: over(h.depth) },
                        Part { factor: Factor::StatusAccumulator, amount: acc, head: h.unit,
                            head_factor: None, of: over(DEPTH_HIT) },
                    ]
                });
                (seeds + acc, false, true, DamageType::Heat, DamageType::Heat, None)
            }
            Ev::Blast(i) => {
                seeded_by = u32::MAX;
                // Consolidated like Heat, and unattributable for the same
                // reason.
                dot_owner = Seat::WIELDER;
                // ONE MOMENT, HOWEVER MANY STACKS SHARE IT. Two applied by the
                // same shot carry the same fuse and go off together; an arcane
                // counting hits sees one, the same way it sees one for a
                // shotgun's pellets.
                if arc.blast_pops.last() != Some(&now) {
                    arc.blast_pops.push(now);
                    r.blast_pops += 1;
                }
                let b = debuffs.blast.remove(*i);
                r.blast_gauge_hits += u32::from(b.charges_gauge);
                dot_parts = rec.is_on().then(|| vec![b.seed_part(rec)]);
                (
                    b.value,
                    false,
                    false,
                    DamageType::Blast,
                    DamageType::Blast,
                    Some(b.xh_bracket),
                )
            }
        };

        // SECONDARY FORTIFIER REACHES A TICK TOO, decided by `apply` as the
        // tick lands: measured (M120), a Heat tick reads ×8 while the
        // Overguard holds and its plain value once it is gone. Once, not
        // squared — a tick is the same instance's payload landing later, not a
        // derivation, so nothing re-applies it the way faction is.
        let mut breakdown = watching(rec);
        let settled = target.apply(
            value,
            TypeShares::single(hit_type),
            false,
            now,
            p,
            ignores_armor,
            &mit,
            params.overguard_multiplier(),
            breakdown.as_mut(),
        );
        let (effective, killed, broke) = (settled.effective, settled.killed, settled.broken);
        // …AND WHOSE BODY IT WAS. Only worth recording once there is more than
        // one to tell apart, which is the same rule the direct hit follows.
        r.sources.add_status(src, effective);
        let was = rec.attribute_to((seeded_by != u32::MAX).then_some(seeded_by));
        ledger::settle(
            r, rec, now, dot_owner, body_index, || src,
            if src == DamageType::Blast { PopKind::Blast } else { PopKind::Status },
            breakdown.as_ref(), settled, Some(debuffs),
            ledger::Clock::Dot,
            || Instance {
                origin: crate::record::Origin::Status,
                head: tick_landing.head,
                // EVERY SEED DOWN TO THE HIT'S MODIFIED BASE. The hit's own
                // half — coefficient, status damage, crit, body part — was kept
                // by the record when the proc landed (`record::Seed`), so the
                // tick draws it beside what it re-reads live; `Event::cause`
                // still names the shot that seeded it.
                //
                // THE ACCUMULATOR IS ITS OWN PART: the tick group's own 1
                // (MEASUREMENTS M58), small, and the thing a reader checking a
                // closed form will be off by.
                base: native_layers.as_ref().and_then(|layers| match layers.first() {
                    Some(crate::record::Layer::Sum { out, .. }) => Some(*out), _ => None,
                }).unwrap_or(value),
                layers: native_layers.unwrap_or_else(|| dot_parts
                    .take()
                    .map(|parts| {
                        let out: f64 = parts.iter().map(|x| x.amount).sum();
                        vec![crate::record::Layer::Sum {
                            factor: crate::record::Factor::StatusSeeds,
                            parts,
                            out,
                        }]
                    })
                    .unwrap_or_default()),
                ..Instance::default()
            },
        );
        r.dot_ticks += is_dot_tick as u32;
        if killed {
            r.note_kills(1, now, params.drop_is_in_reach(target.at));
        }
        if let Some(pool) = broke.filter(|_| !killed) {
            if push_break_proc(debuffs, dot_owner, params, now, pool, active, foe,
                target, gal, arc, r, rec, rng, 0.0, body_index) {
                rec.attribute_to(was);
                break;
            }
        }
        rec.attribute_to(was);
        if killed {
            // Ordinary kill effects include status kills. Electricity/Gas
            // also keep their latest application's weak-point flag (M100).
            // GAS TOO, and it is the one that had to be asked: the cloud is an
            // entity with a radius that outlives its host, so "the cloud's kill
            // rather than the weapon's" is a reading the shape invites and the
            // game refuses (MEASUREMENTS M70).
            gal.bump_on_kill(params, now);
            arc.on_kill(params, now, params.player_distance_m(body_index, target.at));
            if tick_landing.weak_point {
                weakpoint_kill(params, arc, w, now);
            }
            // Fresh individual: clean DebuffBar.
            debuffs.on_death(dot_owner, params.acid_shells, &params.foe);
            break;
        }
        // …and the detonation's EXTRA HIT, off the detonation's value. No body
        // part: a detonation struck none.
        if let Some(bracket) = xh {
            if fire_extra_hits(
                dot_owner,
                value,
                bracket,
                1.0,
                true,
                false,
                active.status_chance,
                now,
                body_index,
                debuffs,
                gal,
                arc,
                target,
                params,
                active,
                &mit,
                r,
                rec,
                rng,
            ) {
                break;
            }
        }

        // KEEP THE QUEUE HONEST. A queued event names its slot by INDEX, so
        // anything that RESHAPES the collections invalidates every entry — a
        // Blast detonation removes itself, and a detonation's own procs can
        // push a DoT and evict another. The shape is DERIVED rather than
        // announced by each mutation site (`tick_shape`), so a site added later
        // cannot forget to say so: the loop simply looks.
        //
        // Otherwise the event that just fired goes back in at its new time,
        // which is what makes this O(log n) instead of a fresh scan.
        if !use_queue {
            continue;   // the scan re-reads the truth every time; nothing to keep
        }
        let now_shape = debuffs.tick_shape();
        if now_shape.0 != shape.0 || now_shape.1 != shape.1 || (now_shape.2 != shape.2 && any_dot_expired) {
            unqueued_due = debuffs.refill_tick_queue(&mut q, until);
            advanced_due = f64::INFINITY;
            can_skip_retain = false;
            shape = now_shape;
        } else if now_shape.2 != shape.2 {
            q.retain(|std::cmp::Reverse(k)| k.class != 2);
            for (bi, b) in debuffs.blast.iter().enumerate() {
                if b.fuse < until {
                    q.push(std::cmp::Reverse(TickKey { t: b.fuse, class: 2, index: bi as u32 }));
                }
            }
            shape = now_shape;
        } else {
            match &ev {
                Ev::Dot(i) => {
                    let d = &debuffs.dots[*i];
                    if d.ticks_left > 0 && d.next_tick < until {
                        q.push(std::cmp::Reverse(TickKey {
                            t: d.next_tick, class: 0, index: *i as u32,
                        }));
                    }
                }
                Ev::Heat => {
                    if let Some(h) = &debuffs.heat {
                        if h.next_tick <= h.expiry && h.next_tick < until {
                            q.push(std::cmp::Reverse(TickKey {
                                t: h.next_tick, class: 1, index: 0,
                            }));
                        }
                    }
                }
                Ev::Blast(_) => {}
            }
        }
    }
    // BACK WHERE IT LIVES, with its capacity — see `tick_q`.
    q.clear();
    debuffs.tick_q = q;
    if !any_dot_expired
        && !(debuffs.dot_tombstones >= 64 && (debuffs.dot_tombstones as usize) * 2 >= debuffs.dots.len())
        && debuffs.live_dot_types != u32::MAX
        && debuffs.dots_gen == initial_dots_gen
    {
        if debuffs.has_fast_dot_schedule() {
            debuffs.dots_due = Some(debuffs.fast_dots_due());
            return;
        }
        if use_queue && can_skip_retain && advanced_due >= until {
            debuffs.dots_due = Some(unqueued_due.min(advanced_due));
            return;
        }
    }
    debuffs.dots_due = Some(debuffs.sweep_expired_dots());
}

/// WHAT IS STILL IN THE AIR, settled up to this shot — every entity already
/// out there, the armour the kills since the last shot stripped,
/// and the packs they dropped.
///
/// A SHOT BOUNDARY IS THE CLOCK every one of these is read on. None of them
/// is fired by the trigger, and each carries the same buff snapshot the shot
/// about to go off does.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn settle_what_is_in_the_air(
    // Whose entities, under whose cards, and the state they move — the orb its
    // meter throws is that seat's too.
    turn: &mut Turn,
    rec: &mut crate::record::Record,
    t: &mut f64,
    entities: &mut Vec<Entity>,
    meter: &mut Meter,
    ammo: &mut Ammo,
    bodies: &mut [Body],
    r: &mut RunResult,
    strip_kills_seen: &mut u32,) {
        let (params, active, owner) = (turn.params, turn.active, turn.seat);
        // A MAGAZINE THROWN BY THE RELOAD BEFORE THIS SHOT, settled at the
        // instant it left — what is due before it acts first, the rest after.
        enqueue_reload_effects(ammo, owner, entities);
        process_entities(entities, *t, turn, r, rec, bodies);
        // JAHU CANTICLE. Every kill takes a share off the armour of every enemy
        // inside Affinity Range — which is measured from the PLAYER, not from
        // the corpse (wiki `Affinity`: the squad shares within a 50 m radius),
        // so which body died does not matter and a count is enough.
        //
        // THE SHARES COMPOSE rather than adding: each kill removes a share of
        // what is LEFT, which is the rule every other strip in this engine
        // follows and the only one under which repeated kills cannot take
        // armour past zero. Two kills at 5% leave 0.9025 of it, not 0.90.
        //
        // NO CLOCK. The card states no duration, so what it takes it keeps.
        if let Some((share, radius)) = active.strip_on_kill_in_range {
            let fresh = r.kills.saturating_sub(*strip_kills_seen);
            *strip_kills_seen = r.kills;
            if fresh > 0 && share > 0.0 {
                let keep = (1.0 - share).powi_portable(fresh as i32);
                for (b, foe) in bodies.iter_mut().enumerate() {
                    let Some(spec) = params.body(b) else { continue };
                    if !crate::rules::space::within(params.player_distance_m(b, spec.at), radius) {
                        continue;
                    }
                    let fd = &mut foe.debuffs;
                    fd.canticle_armor_strip = 1.0 - (1.0 - fd.canticle_armor_strip) * keep;
                }
            }
        }
        // WHAT THE BODIES DROPPED since this was last looked at — ONE roll per
        // kill IN REACH, read by everything that cares (docs/MECHANICS.md
        // §"THE AMMO ECONOMY"). Rolled whether or not anything reads it, which
        // is what keeps two builds of one weapon on the same dice.
        let (mut dropped_primary, mut dropped_secondary) = (0u32, 0u32);
        for _ in 0..r.kills_in_reach.saturating_sub(ammo.drop_kill_mark) {
            let (p, s) = crate::rules::ammo::on_kill(
                params.squad_size,
                params.landscape,
                params.foe.eximus,
                &mut turn.d.drops,
            );
            dropped_primary += p;
            dropped_secondary += s;
        }
        ammo.drop_kill_mark = r.kills_in_reach;
        // …AND WHAT THIS WEAPON DOES WITH THEM (`rules::ammo::credit`). Nothing at all
        // while the reserve is infinite: the house rule already hands the
        // weapon everything a pack could.
        ammo.credit_pickups(params, r, dropped_primary, dropped_secondary);
        // THE RECHARGE METER, credited with the seconds since it was last
        // looked at. A shot boundary is where every other clock in this loop is
        // read, and the meter is coarse enough not to care: it is 45 seconds
        // long and the fastest thing that fills it is worth one.
        if let Some(m) = active.meter {
            meter.tick(m, owner, active, params, dropped_secondary, t, entities);
        }
        // …AND WHAT THE METER THREW, on the same boundary and the same buff
        // snapshot. An orb's clock is its own and no fire-rate bucket reaches it,
        // which is the wiki's *"Tick rate is not affected by Fire Rate"* holding
        // by construction.
        process_entities(entities, *t, turn, r, rec, bodies);
}
