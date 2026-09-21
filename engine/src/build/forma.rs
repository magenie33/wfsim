// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE FORMA PLANNER: one polarity layout for one or more loadouts of the SAME
//! item, under the player's own rules (docs/INVESTMENT.md §The planner).
//!
//! Polarity belongs to the item and every config on it shares it, while a
//! config may put its mods in any slot. So the question is a MULTISET of
//! polarities that every loadout fits into, and it is answered by trying them
//! all: the alphabet is the handful of colours the loadouts and the item carry,
//! which keeps the enumeration to thousands, and an exhaustive answer is one
//! that needs no second search to vouch for it.

use crate::rules::capacity::{capacity, forma_to_max_rank, rank_after, slot_drain, FormaCost, Polarity};

/// Whether the planner may buy an Omni Forma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmniUse {
    Never,
    /// Only where it saves a Forma — it is the costlier item.
    Allowed,
    /// Every polarization bought is Omni; innate colours are still kept.
    Preferred,
}

/// Whether the planner may buy an Umbra Forma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UmbraUse {
    /// Never bought: an Umbra mod pays full or mismatched drain.
    Never,
    /// As few as possible, but used rather than fail — Umbra is the scarce
    /// item, so a layout with fewer of them wins before Forma are counted.
    WhenNeeded,
    /// An ordinary Forma: counted with the rest.
    Allowed,
}

/// THE PLAYER'S RULES — global, not per item and not per build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    /// Orokin Catalyst / Reactor: doubles capacity.
    pub catalyst: bool,
    /// Spend at least what the item's max rank takes (five on a rank-40 weapon),
    /// because full mastery affinity takes them whether the build needs them.
    pub reach_max_rank: bool,
    /// Once any Forma is spent, match the stance/aura slot first: it buys
    /// capacity outright, which beats halving any drain of ten or less. Costs at
    /// most one Forma over the minimum, and a layout that is free stays free.
    pub grant_slot_first: bool,
    pub omni: OmniUse,
    pub umbra: UmbraUse,
    /// The most Forma the bill may come to, mastery ones included.
    pub forma_limit: Option<u32>,
    /// Mods never move: each slot's colour serves whatever every loadout keeps
    /// there. Off, mods are moved onto the layout, ordered cards in order.
    pub fixed_order: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            catalyst: true,
            reach_max_rank: true,
            grant_slot_first: true,
            omni: OmniUse::Never,
            umbra: UmbraUse::WhenNeeded,
            forma_limit: None,
            fixed_order: false,
        }
    }
}

/// A slot that GRANTS capacity rather than taking it: a weapon's stance slot or
/// a Warframe's aura slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantSlot {
    pub innate: Option<Polarity>,
    /// Does its polarity swap with the other slots'? A Warframe's aura slot
    /// does; a weapon's stance slot is a slot of its own.
    pub in_pool: bool,
    /// Takes no Forma at all — a stance the weapon cannot take off (M94).
    pub fixed: bool,
}

/// The item being polarized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub base_max_rank: u32,
    /// Innate polarity of each main slot; the length is the slot count.
    pub main: Vec<Option<Polarity>>,
    /// `None`: the item has no exilus slot.
    pub exilus: Option<Option<Polarity>>,
    pub grant: Option<GrantSlot>,
}

impl Board {
    /// A weapon from the roster.
    pub fn weapon(id: &str) -> Option<Self> {
        use crate::data::weapons as w;
        let spec = w::spec(id)?;
        Some(Self {
            base_max_rank: spec.max_rank,
            main: w::innate_slots(id).to_vec(),
            exilus: w::has_exilus_slot(id).then(|| w::exilus_polarity(id)),
            grant: (spec.slot == "melee").then(|| GrantSlot {
                innate: w::stance_polarity(id),
                in_pool: false,
                fixed: spec.fixed_stance.is_some(),
            }),
        })
    }

    /// A Warframe: rank 30, eight main slots, an exilus and an aura slot whose
    /// polarities are one pool with the rest.
    pub fn warframe(frame: &crate::data::warframes::WarframeDef) -> Self {
        use crate::data::weapons::polarity;
        let mut main: Vec<Option<Polarity>> =
            frame.polarities.iter().take(8).map(|p| Some(polarity(p))).collect();
        main.resize(8, None);
        Self {
            base_max_rank: 30,
            main,
            exilus: Some(frame.exilus_polarity.as_deref().map(polarity)),
            grant: Some(GrantSlot {
                innate: frame.aura_polarity.as_deref().map(polarity),
                in_pool: true,
                fixed: false,
            }),
        }
    }

    pub fn innate(&self) -> Layout {
        Layout {
            main: self.main.clone(),
            exilus: self.exilus.flatten(),
            grant: self.grant.and_then(|g| g.innate),
        }
    }
}

/// A card as the planner sees it: its drain at the rank it is set to, or for a
/// grant card (stance, aura) the capacity it hands back on a bare slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub drain: u32,
    pub polarity: Polarity,
    /// Its place relative to the other ordered cards is part of the build —
    /// an element-bearing mod, whose order decides what pairs.
    pub ordered: bool,
}

/// One config. `main` is positional — `None` is an empty slot — so the first
/// loadout's mods keep the slots they are in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loadout {
    pub main: Vec<Option<Card>>,
    pub exilus: Option<Card>,
    pub grant: Option<Card>,
}

/// A polarity per slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub main: Vec<Option<Polarity>>,
    pub exilus: Option<Polarity>,
    pub grant: Option<Polarity>,
}

/// What the item already carries, when the player says so: the layout and how
/// many Forma it has taken (which is what its max rank already reflects).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub layout: Layout,
    pub forma_spent: u32,
    /// The slots stay where they are: a layout made of the same colours is
    /// placed exactly as the start has them, and only mods move.
    pub pinned: bool,
}

/// One loadout placed on the planned layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// Per main slot, the index into the loadout's `main` of the mod put there.
    pub slots: Vec<Option<usize>>,
    pub drain: u32,
    /// What its grant card hands back on the planned grant slot.
    pub grant: u32,
    /// `capacity + grant - drain` — below zero only in [`closest`].
    pub spare: i32,
    /// Mods not in the slot they came in.
    pub moved: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub layout: Layout,
    /// The bill for THIS plan, mastery Forma included.
    pub cost: FormaCost,
    pub rank: u32,
    /// Before any grant.
    pub capacity: u32,
    /// Index-aligned with the loadouts handed in.
    pub loadouts: Vec<Placed>,
    /// False when the work budget ran out before every layout was tried.
    pub exhaustive: bool,
}

/// Why there is no plan — the one case the page has to push back, so it names
/// the cause in a shape the page can word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The request itself cannot describe this item.
    Invalid(String),
    /// A plan exists and costs more than the player's limit.
    OverLimit { need: u32, limit: u32 },
    /// These loadouts (0-based) do not fit even alone and fully polarized.
    DoesNotFit { loadouts: Vec<usize>, umbra_off: bool },
    /// Each fits alone; no one layout serves them all.
    CannotShare,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(e) => f.write_str(e),
            Self::OverLimit { need, limit } => write!(f, "needs {need} Forma, over the limit of {limit}"),
            Self::DoesNotFit { loadouts, umbra_off } => write!(
                f,
                "loadout {} does not fit even fully polarized{}",
                loadouts.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", "),
                if *umbra_off { " while Umbra Forma is off" } else { "" }
            ),
            Self::CannotShare => f.write_str("these loadouts cannot share one polarity layout"),
        }
    }
}

fn check_loadout(board: &Board, i: usize, l: &Loadout) -> Result<(), PlanError> {
    let n = board.main.len();
    let bad = |e: String| Err(PlanError::Invalid(e));
    let mods = l.main.iter().flatten().count();
    if l.main.len() > n {
        return bad(format!("loadout {}: {} slots given, the item has {n}", i + 1, l.main.len()));
    }
    if mods > n {
        return bad(format!("loadout {}: {mods} mods for {n} slots", i + 1));
    }
    if l.exilus.is_some() && board.exilus.is_none() {
        return bad(format!("loadout {}: the item has no exilus slot", i + 1));
    }
    if l.grant.is_some() && board.grant.is_none() {
        return bad(format!("loadout {}: the item has no stance or aura slot", i + 1));
    }
    Ok(())
}

fn check_board(board: &Board, start: Option<&Start>) -> Result<(), PlanError> {
    let n = board.main.len();
    if n > MAX_SLOTS {
        return Err(PlanError::Invalid(format!("{n} slots is more than the planner takes ({MAX_SLOTS})")));
    }
    if let Some(s) = start {
        if s.layout.main.len() != n {
            return Err(PlanError::Invalid(format!(
                "the current layout has {} main slots, the item has {n}",
                s.layout.main.len()
            )));
        }
    }
    Ok(())
}

/// Plan one layout for every loadout — every one of them must fit.
///
/// The order a layout is judged in: Umbra Forma (under `WhenNeeded`), then the
/// grant slot (under `grant_slot_first`), then Forma, then Omni among equal
/// Forma, then the WORST loadout's spare capacity, then the total, then the
/// fewest mods moved, then the fewest of the item's own colours moved.
pub fn plan(
    board: &Board,
    loadouts: &[Loadout],
    rules: Rules,
    start: Option<&Start>,
) -> Result<Plan, PlanError> {
    check_board(board, start)?;
    if loadouts.is_empty() {
        return Err(PlanError::Invalid("no loadout to plan for".into()));
    }
    for (i, l) in loadouts.iter().enumerate() {
        check_loadout(board, i, l)?;
    }
    let innate = Start { layout: board.innate(), forma_spent: 0, pinned: false };
    let start = start.unwrap_or(&innate);
    let refs: Vec<&Loadout> = loadouts.iter().collect();
    let solve = |rules: Rules, refs: &[&Loadout]| {
        Space::new(board, refs, rules, start).best(refs, false, &mut Budget::default())
    };
    if let Some(p) = solve(rules, &refs) {
        return Ok(p);
    }
    if let Some(limit) = rules.forma_limit {
        if let Some(p) = solve(Rules { forma_limit: None, ..rules }, &refs) {
            return Err(PlanError::OverLimit { need: p.cost.total(), limit });
        }
    }
    let alone: Vec<usize> = (0..refs.len()).filter(|&i| solve(rules, &refs[i..=i]).is_none()).collect();
    if alone.is_empty() {
        return Err(PlanError::CannotShare);
    }
    let umbra_off = rules.umbra == UmbraUse::Never
        && alone.iter().any(|&i| {
            let l = &loadouts[i];
            l.main.iter().flatten().chain(&l.exilus).any(|c| c.polarity == Polarity::Umbra)
        });
    Err(PlanError::DoesNotFit { loadouts: alone, umbra_off })
}

/// THE NEAREST MISS, for a loadout no layout fits: the layout that leaves the
/// worst loadout least far over, whatever it costs and ignoring the limit. The
/// page shows it so the reader sees how far over the build is rather than an
/// unpolarized one.
pub fn closest(board: &Board, loadouts: &[Loadout], rules: Rules, start: Option<&Start>) -> Option<Plan> {
    check_board(board, start).ok()?;
    let innate = Start { layout: board.innate(), forma_spent: 0, pinned: false };
    let start = start.unwrap_or(&innate);
    let refs: Vec<&Loadout> = loadouts.iter().collect();
    Space::new(board, &refs, Rules { forma_limit: None, ..rules }, start).best(&refs, true, &mut Budget::default())
}

/// A build on a board, and how close it comes to its group's leader.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupBuild {
    pub loadout: Loadout,
    /// Its score over the group leader's, 0..=1.
    pub ratio: f64,
}

/// A scenario: covered when ONE of its builds fits the layout.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    pub builds: Vec<GroupBuild>,
}

/// The build a layout serves a group with.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    /// Index into the group's `builds` as handed in.
    pub build: usize,
    pub ratio: f64,
    pub placed: Placed,
}

/// One point on the Forma-to-coverage curve: the cheapest layout that reaches
/// its `worst` ratio.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    /// The layout, with the hard loadouts placed on it.
    pub plan: Plan,
    /// The weakest group's best fitting ratio; 0 where a group has none.
    pub worst: f64,
    /// Per group.
    pub picks: Vec<Option<Pick>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    /// Cheapest first, each point strictly better than the one before it.
    pub curve: Vec<Point>,
    /// False when the work budget ran out before every layout was tried.
    pub exhaustive: bool,
}

/// A build's cards as far as a layout can tell two builds apart.
type Shape = (Vec<(u32, usize, bool)>, Option<Card>, Option<Card>);

/// THE OPTIMIZER OF THE PLAN: what each Forma buys across whole groups of
/// builds. Every `hard` loadout must fit every point; a group counts through
/// the best of its builds that fits. Builds under `floor` are not read.
pub fn optimize(
    board: &Board,
    hard: &[Loadout],
    groups: &[Group],
    floor: f64,
    rules: Rules,
    start: Option<&Start>,
) -> Result<Coverage, PlanError> {
    check_board(board, start)?;
    if groups.iter().all(|g| g.builds.is_empty()) {
        return Err(PlanError::Invalid("no build to plan for".into()));
    }
    for (i, l) in hard.iter().enumerate() {
        check_loadout(board, i, l)?;
    }
    if !hard.is_empty() {
        plan(board, hard, Rules { forma_limit: None, ..rules }, start)?;
    }
    let innate = Start { layout: board.innate(), forma_spent: 0, pinned: false };
    let start = start.unwrap_or(&innate);

    // EACH GROUP: its builds over the floor, one per shape, best first.
    let mut kept: Vec<Vec<(usize, &Loadout, f64)>> = Vec::new();
    for g in groups {
        let mut seen: Vec<Shape> = Vec::new();
        let mut rows: Vec<(usize, &Loadout, f64)> = g
            .builds
            .iter()
            .enumerate()
            .filter(|(i, b)| b.ratio >= floor && check_loadout(board, *i, &b.loadout).is_ok())
            .map(|(i, b)| (i, &b.loadout, b.ratio))
            .collect();
        rows.sort_by(|a, b| b.2.total_cmp(&a.2));
        rows.retain(|(_, l, _)| {
            let mut shape: Vec<(u32, usize, bool)> = l
                .main
                .iter()
                .enumerate()
                .filter_map(|(p, c)| c.map(|c| (c.drain, sym(Some(c.polarity)) * 16 + if rules.fixed_order { p } else { 0 }, c.ordered)))
                .collect();
            if !rules.fixed_order {
                shape.sort_unstable();
            }
            let key = (shape, l.exilus, l.grant);
            if seen.contains(&key) {
                return false;
            }
            seen.push(key);
            true
        });
        kept.push(rows);
    }

    let mut all: Vec<&Loadout> = hard.iter().collect();
    let base = all.len();
    let mut at: Vec<Vec<usize>> = Vec::new();
    for rows in &kept {
        at.push(rows.iter().map(|(_, l, _)| {
            all.push(l);
            all.len() - 1
        }).collect());
    }
    let space = Space::new(board, &all, rules, start);
    let mut memo = Memo::new(all.len(), space.mains.len());
    let prepared: Vec<Prepared> = all.iter().map(|l| Prepared::of(l)).collect();
    let mut budget = Budget::default();

    // RELAXED: positions free. It never under-rates a layout, so it orders the
    // exact work and bounds it.
    let mut by_total: std::collections::BTreeMap<u32, Vec<(f64, usize)>> = Default::default();
    for (ci, c) in space.cands.iter().enumerate() {
        if (0..base).any(|li| space.relaxed_spare(c, all[li], &prepared[li], li, &mut memo) < 0) {
            continue;
        }
        let mut worst = f64::INFINITY;
        for (gi, rows) in kept.iter().enumerate() {
            let hit = rows
                .iter()
                .zip(&at[gi])
                .find(|(_, &li)| space.relaxed_spare(c, all[li], &prepared[li], li, &mut memo) >= 0)
                .map_or(0.0, |((_, _, r), _)| *r);
            worst = worst.min(hit);
            if worst <= 0.0 {
                break;
            }
        }
        if worst.is_finite() {
            by_total.entry(c.cost.total()).or_default().push((worst, ci));
        }
    }

    let ceiling = kept.iter().map(|r| r.first().map_or(0.0, |x| x.2)).fold(f64::INFINITY, f64::min);
    let mut curve: Vec<Point> = Vec::new();
    for (_, mut list) in by_total {
        let floor_now = curve.last().map_or(-1.0, |p| p.worst);
        list.sort_by(|a, b| {
            b.0.total_cmp(&a.0).then_with(|| space.cands[a.1].key.cmp(&space.cands[b.1].key))
        });
        let mut best: Option<Point> = None;
        for &(relaxed, ci) in &list {
            if relaxed <= floor_now || best.as_ref().is_some_and(|p| relaxed <= p.worst) {
                break;
            }
            let c = &space.cands[ci];
            // Position for the hard loadouts and the builds the relaxed walk chose.
            let mut loads: Vec<&Loadout> = all[..base].to_vec();
            for (gi, rows) in kept.iter().enumerate() {
                if let Some((_, &li)) = rows
                    .iter()
                    .zip(&at[gi])
                    .find(|(_, &li)| space.relaxed_spare(c, all[li], &prepared[li], li, &mut memo) >= 0)
                {
                    loads.push(all[li]);
                }
            }
            let Some(r) = space.realize(c, &loads, &mut budget) else { continue };
            // EXACT: every group walked again on the positioned layout.
            let mut picks = Vec::new();
            let mut worst = f64::INFINITY;
            for rows in &kept {
                let hit = rows.iter().find_map(|(bi, l, ratio)| {
                    let placed = space.place(c, &r.syms, l);
                    (placed.spare >= 0).then_some(Pick { build: *bi, ratio: *ratio, placed })
                });
                worst = worst.min(hit.as_ref().map_or(0.0, |p| p.ratio));
                picks.push(hit);
            }
            if worst > floor_now && best.as_ref().is_none_or(|p| worst > p.worst) {
                best = Some(Point { plan: space.plan_of(c, &r, base), worst, picks });
            }
        }
        if let Some(p) = best {
            let done = p.worst >= ceiling;
            curve.push(p);
            if done {
                break;
            }
        }
    }
    Ok(Coverage { curve, exhaustive: !budget.spent })
}

const MAX_SLOTS: usize = 12;
const NSYM: usize = 10;
/// A polarity as an index; 0 is a bare slot.
const SYMS: [Option<Polarity>; NSYM] = [
    None,
    Some(Polarity::Madurai),
    Some(Polarity::Naramon),
    Some(Polarity::Vazarin),
    Some(Polarity::Zenurik),
    Some(Polarity::Unairu),
    Some(Polarity::Penjaga),
    Some(Polarity::Umbra),
    Some(Polarity::Aura),
    Some(Polarity::Omni),
];
const BLANK: usize = 0;
const UMBRA: usize = 7;
const AURA: usize = 8;
const OMNI: usize = 9;
const REGULAR: std::ops::RangeInclusive<usize> = 1..=6;

fn sym(p: Option<Polarity>) -> usize {
    SYMS.iter().position(|&s| s == p).expect("every polarity has a symbol")
}

type Counts = [i32; NSYM];

/// What a grant card hands back on a slot (wiki `Aura`, and `Stance` for the
/// same arithmetic): double on its own colour, the listed figure on a bare
/// slot, 80% rounded down on another.
fn grant_on(card: Card, slot: Option<Polarity>) -> u32 {
    match slot {
        None => card.drain,
        Some(p) if p == card.polarity || p == Polarity::Omni => card.drain * 2,
        Some(_) => card.drain * 4 / 5,
    }
}

fn grant_matched(card: Card, slot: Option<Polarity>) -> bool {
    matches!(slot, Some(p) if p == card.polarity || p == Polarity::Omni)
}

fn cost_on(c: Card, k: usize) -> u32 {
    slot_drain(c.drain, c.polarity, SYMS[k])
}

/// Minimum-cost assignment of `rows` to distinct `cols` (rows ≤ cols), the
/// Hungarian method. Returns the total and the column of each row.
fn assign(rows: usize, cols: usize, cost: impl Fn(usize, usize) -> i64) -> (i64, [usize; MAX_SLOTS]) {
    let mut out = [0usize; MAX_SLOTS];
    if rows == 0 {
        return (0, out);
    }
    const INF: i64 = i64::MAX / 4;
    let mut u = [0i64; MAX_SLOTS + 1];
    let mut v = [0i64; MAX_SLOTS + 1];
    let mut p = [0usize; MAX_SLOTS + 1];
    let mut way = [0usize; MAX_SLOTS + 1];
    for i in 1..=rows {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = [INF; MAX_SLOTS + 1];
        let mut used = [false; MAX_SLOTS + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let (mut delta, mut j1) = (INF, 0);
            for j in 1..=cols {
                if !used[j] {
                    let cur = cost(i0 - 1, j - 1) - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=cols {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut total = 0;
    for j in 1..=cols {
        if p[j] != 0 {
            out[p[j] - 1] = j - 1;
            total += cost(p[j] - 1, j - 1);
        }
    }
    (total, out)
}

/// THE FEWEST DRAIN a set of cards can take on a multiset of slots, positions
/// free. Each colour's slots go to that colour's biggest cards, Omni to the
/// biggest left that are not Umbra, bare slots to the biggest after that and
/// the rest mismatch — `free_drain_is_the_optimal_assignment` holds it to the
/// Hungarian answer. `sorted` is (drain, symbol), biggest first.
fn free_drain(sorted: &[(u32, u8)], syms: &[u8]) -> u32 {
    let mut left = [0u8; NSYM];
    for &k in syms {
        left[k as usize] += 1;
    }
    let mut total = 0;
    let mut rest = [(0u32, 0u8); MAX_SLOTS];
    let mut nr = 0;
    for &(d, p) in sorted {
        if left[p as usize] > 0 {
            left[p as usize] -= 1;
            total += d.div_ceil(2);
        } else {
            rest[nr] = (d, p);
            nr += 1;
        }
    }
    let mut blanks = left[BLANK];
    for &(d, p) in &rest[..nr] {
        if p as usize != UMBRA && left[OMNI] > 0 {
            left[OMNI] -= 1;
            total += d.div_ceil(2);
        } else if blanks > 0 {
            // Biggest first, so the bare slots take the biggest of what is left.
            blanks -= 1;
            total += d;
        } else {
            total += slot_drain(d, Polarity::Madurai, Some(Polarity::Naramon));
        }
    }
    total
}

/// A loadout with its main cards sorted for [`free_drain`].
struct Prepared {
    sorted: Vec<(u32, u8)>,
}

impl Prepared {
    fn of(l: &Loadout) -> Self {
        let mut sorted: Vec<(u32, u8)> =
            l.main.iter().flatten().map(|c| (c.drain, sym(Some(c.polarity)) as u8)).collect();
        sorted.sort_unstable_by_key(|x| std::cmp::Reverse(x.0));
        Self { sorted }
    }
}

/// Free drains already worked out, per loadout and main multiset.
struct Memo {
    rows: Vec<Vec<u16>>,
    width: usize,
}

impl Memo {
    fn new(loadouts: usize, width: usize) -> Self {
        Self { rows: vec![Vec::new(); loadouts], width }
    }
}

/// How much exact work a search may do before it settles for what it has.
struct Budget {
    left: u64,
    spent: bool,
}

impl Default for Budget {
    fn default() -> Self {
        Self { left: 4_000_000, spent: false }
    }
}

impl Budget {
    fn take(&mut self, n: u64) -> bool {
        if self.left < n {
            self.spent = true;
            return false;
        }
        self.left -= n;
        true
    }
}

/// Every increasing `k`-subset of `0..n`.
fn combinations(n: usize, k: usize, f: &mut dyn FnMut(&[usize])) {
    fn go(from: usize, n: usize, k: usize, cur: &mut Vec<usize>, f: &mut dyn FnMut(&[usize])) {
        if cur.len() == k {
            f(cur);
            return;
        }
        for i in from..n {
            if n - i < k - cur.len() {
                break;
            }
            cur.push(i);
            go(i + 1, n, k, cur, f);
            cur.pop();
        }
    }
    go(0, n, k, &mut Vec::with_capacity(k), f);
}

/// A loadout's main cards on a positioned layout: which mod goes where, what
/// it drains, and how many mods moved. Each card is placed freely except that
/// the ORDERED ones keep their order among themselves, which is what keeps an
/// element pairing what it was. With `fixed` nothing moves.
fn place_main(cards: &[Option<Card>], syms: &[u8], fixed: bool) -> (u32, Vec<Option<usize>>, u32) {
    let n = syms.len();
    let mut slots = vec![None; n];
    if fixed {
        let mut drain = 0;
        for (p, c) in cards.iter().enumerate() {
            if let Some(c) = c {
                drain += cost_on(*c, syms[p] as usize);
                slots[p] = Some(p);
            }
        }
        return (drain, slots, 0);
    }
    let mut ordered: Vec<(usize, Card)> = Vec::new();
    let mut free: Vec<(usize, Card)> = Vec::new();
    for (p, c) in cards.iter().enumerate() {
        if let Some(c) = c {
            if c.ordered { ordered.push((p, *c)) } else { free.push((p, *c)) }
        }
    }
    if ordered.len() <= 1 {
        free.append(&mut ordered);
    }
    // A move costs one sixteenth of a point, so it only ever breaks a tie.
    let w = |c: &(usize, Card), at: usize| i64::from(cost_on(c.1, syms[at] as usize)) * 16 + i64::from(c.0 != at);
    let mut best = (i64::MAX, Vec::new());
    combinations(n, ordered.len(), &mut |pos| {
        let own: i64 = ordered.iter().zip(pos).map(|(c, &at)| w(c, at)).sum();
        if own >= best.0 {
            return;
        }
        let rest: Vec<usize> = (0..n).filter(|p| !pos.contains(p)).collect();
        let (total, to) = assign(free.len(), rest.len(), |r, col| w(&free[r], rest[col]));
        if own + total < best.0 {
            let mut at: Vec<(usize, usize)> = ordered.iter().zip(pos).map(|(c, &p)| (p, c.0)).collect();
            at.extend(free.iter().enumerate().map(|(r, c)| (rest[to[r]], c.0)));
            best = (own + total, at);
        }
    });
    for &(slot, from) in &best.1 {
        slots[slot] = Some(from);
    }
    ((best.0 / 16) as u32, slots, (best.0 % 16) as u32)
}

/// One candidate: a main multiset, the exilus and grant colours, and its bill.
#[derive(Debug, Clone)]
struct Cand {
    key: [u32; 4],
    mi: u32,
    e: u8,
    g: u8,
    cost: FormaCost,
    rank: u32,
    capacity: u32,
}

/// A candidate given positions.
struct Realized {
    syms: Vec<u8>,
    placed: Vec<Placed>,
    score: (i64, i64, i64, i32),
}

/// Every layout the rules allow for these loadouts, billed and sorted.
struct Space<'a> {
    board: &'a Board,
    rules: Rules,
    start: &'a Start,
    n: usize,
    mains: Vec<[u8; MAX_SLOTS]>,
    cands: Vec<Cand>,
}

impl<'a> Space<'a> {
    fn new(board: &'a Board, loadouts: &[&Loadout], rules: Rules, start: &'a Start) -> Self {
        let n = board.main.len();
        let grant_in_pool = board.grant.is_some_and(|g| g.in_pool);
        let s_grant = sym(start.layout.grant);

        // THE START AS A POOL. A bought polarization is one slot of the target
        // multiset the start does not cover — blank counted as a colour of its
        // own, since blanking a slot takes a Forma too — so the bill is
        // Σ max(0, T − S).
        let mut s: Counts = [0; NSYM];
        for &p in &start.layout.main {
            s[sym(p)] += 1;
        }
        if board.exilus.is_some() {
            s[sym(start.layout.exilus)] += 1;
        }
        if grant_in_pool {
            s[s_grant] += 1;
        }

        // THE ALPHABET: a bare slot, what the item already carries, and what
        // some card can match. A colour nobody carries only ever mismatches.
        let mut want = [false; NSYM];
        want[BLANK] = true;
        for (k, &c) in s.iter().enumerate() {
            want[k] |= c > 0;
        }
        for l in loadouts {
            for c in l.main.iter().flatten().chain(&l.exilus) {
                want[sym(Some(c.polarity))] = true;
            }
            if grant_in_pool {
                if let Some(c) = l.grant {
                    want[sym(Some(c.polarity))] = true;
                }
            }
        }
        want[OMNI] |= rules.omni != OmniUse::Never;
        if rules.umbra == UmbraUse::Never && s[UMBRA] == 0 {
            want[UMBRA] = false;
        }
        let alpha: Vec<usize> = (0..NSYM).filter(|&k| want[k]).collect();
        let ex_alpha: Vec<usize> = if board.exilus.is_some() { alpha.clone() } else { vec![BLANK] };
        let g_alpha: Vec<usize> = match board.grant {
            None => vec![BLANK],
            Some(g) if g.fixed => vec![s_grant],
            Some(_) if grant_in_pool => alpha.clone(),
            Some(_) => {
                let mut g = [false; NSYM];
                g[BLANK] = true;
                g[s_grant] = true;
                for c in loadouts.iter().filter_map(|l| l.grant) {
                    g[sym(Some(c.polarity))] = true;
                }
                g[OMNI] |= rules.omni != OmniUse::Never;
                (0..NSYM).filter(|&k| g[k]).collect()
            }
        };

        let mut mains: Vec<[u8; MAX_SLOTS]> = Vec::new();
        fn walk(alpha: &[usize], from: usize, depth: usize, n: usize, cur: &mut [u8; MAX_SLOTS], out: &mut Vec<[u8; MAX_SLOTS]>) {
            if depth == n {
                out.push(*cur);
                return;
            }
            for a in from..alpha.len() {
                cur[depth] = alpha[a] as u8;
                walk(alpha, a, depth + 1, n, cur, out);
            }
        }
        walk(&alpha, 0, 0, n, &mut [0u8; MAX_SLOTS], &mut mains);

        let floor = if rules.reach_max_rank { forma_to_max_rank(board.base_max_rank) } else { 0 };
        let grants: Vec<Card> = loadouts.iter().filter_map(|l| l.grant).collect();
        let mut cands = Vec::new();
        for (mi, m) in mains.iter().enumerate() {
            let mut t: Counts = [0; NSYM];
            for &k in &m[..n] {
                t[k as usize] += 1;
            }
            for &e in &ex_alpha {
                let mut te = t;
                if board.exilus.is_some() {
                    te[e] += 1;
                }
                for &g in &g_alpha {
                    let mut tg = te;
                    if grant_in_pool {
                        tg[g] += 1;
                    }
                    let mut ops = [0i32; NSYM];
                    for k in 0..NSYM {
                        ops[k] = (tg[k] - s[k]).max(0);
                    }
                    if board.grant.is_some() && !grant_in_pool && g != s_grant {
                        ops[g] += 1;
                    }
                    // No Forma makes the Aura colour; only an item born with it has one.
                    if ops[AURA] > 0
                        || (rules.umbra == UmbraUse::Never && ops[UMBRA] > 0)
                        || (rules.omni == OmniUse::Never && ops[OMNI] > 0)
                        || (rules.omni == OmniUse::Preferred && REGULAR.clone().any(|k| ops[k] > 0))
                    {
                        continue;
                    }
                    let mut cost = FormaCost {
                        regular: (ops[BLANK] + REGULAR.clone().map(|k| ops[k]).sum::<i32>()) as u32,
                        omni: ops[OMNI] as u32,
                        umbra: ops[UMBRA] as u32,
                    };
                    let spent = start.forma_spent + cost.total();
                    let extra = floor.saturating_sub(spent);
                    cost.regular += extra;
                    if rules.forma_limit.is_some_and(|l| cost.total() > l) {
                        continue;
                    }
                    let rank = rank_after(board.base_max_rank.max(30), spent + extra);
                    let grant_miss = if rules.grant_slot_first && cost.total() > 0 {
                        grants.iter().filter(|&&c| !grant_matched(c, SYMS[g])).count() as u32
                    } else {
                        0
                    };
                    let key = match rules.umbra {
                        UmbraUse::Allowed => [grant_miss, cost.total(), cost.umbra, cost.omni],
                        _ => [cost.umbra, grant_miss, cost.total(), cost.omni],
                    };
                    cands.push(Cand {
                        key,
                        mi: mi as u32,
                        e: e as u8,
                        g: g as u8,
                        cost,
                        rank,
                        capacity: capacity(rank, rules.catalyst),
                    });
                }
            }
        }
        cands.sort_by_key(|c| c.key);
        Self { board, rules, start, n, mains, cands }
    }

    fn syms(&self, mi: u32) -> &[u8] {
        &self.mains[mi as usize][..self.n]
    }

    fn extras(&self, c: &Cand, l: &Loadout) -> (u32, u32) {
        let ex = l.exilus.map_or(0, |x| cost_on(x, c.e as usize));
        let gr = l.grant.map_or(0, |x| grant_on(x, SYMS[c.g as usize]));
        (ex, gr)
    }

    /// Spare capacity with positions free — never below the exact answer.
    /// `li` names the loadout in `memo`.
    fn relaxed_spare(&self, c: &Cand, l: &Loadout, p: &Prepared, li: usize, memo: &mut Memo) -> i64 {
        let row = &mut memo.rows[li];
        if row.is_empty() {
            row.resize(memo.width, u16::MAX);
        }
        let mi = c.mi as usize;
        if row[mi] == u16::MAX {
            row[mi] = free_drain(&p.sorted, &self.mains[mi][..self.n]) as u16;
        }
        let (ex, gr) = self.extras(c, l);
        i64::from(c.capacity + gr) - i64::from(u32::from(row[mi]) + ex)
    }

    /// One loadout placed on a positioned layout.
    fn place(&self, c: &Cand, syms: &[u8], l: &Loadout) -> Placed {
        let (main, slots, moved) = place_main(&l.main, syms, self.rules.fixed_order);
        let (ex, grant) = self.extras(c, l);
        let drain = main + ex;
        Placed {
            slots,
            drain,
            grant,
            spare: (c.capacity + grant) as i32 - drain as i32,
            moved,
        }
    }

    fn score(&self, c: &Cand, syms: &[u8], loads: &[&Loadout]) -> Realized {
        let placed: Vec<Placed> = loads.iter().map(|l| self.place(c, syms, l)).collect();
        let worst = placed.iter().map(|p| i64::from(p.spare)).min().unwrap_or(0);
        let sum = placed.iter().map(|p| i64::from(p.spare)).sum();
        let moved: i64 = placed.iter().map(|p| i64::from(p.moved)).sum();
        let kept = syms
            .iter()
            .zip(&self.start.layout.main)
            .filter(|(&k, &b)| k as usize == sym(b))
            .count() as i32
            // A colour moved onto or off a slot of its own kind reads as the
            // bigger change of the two.
            - 2 * i32::from(c.e as usize != sym(self.start.layout.exilus))
            - 2 * i32::from(c.g as usize != sym(self.start.layout.grant));
        Realized { syms: syms.to_vec(), placed, score: (worst, sum, -moved, kept) }
    }

    /// POSITIONS for a candidate. Movable: the first loadout's own best
    /// placement anchors the colours, then swaps are taken while they help.
    /// Fixed: every arrangement of the multiset, pruned by capacity.
    fn realize(&self, c: &Cand, loads: &[&Loadout], budget: &mut Budget) -> Option<Realized> {
        let syms = self.syms(c.mi).to_vec();
        if self.start.pinned {
            let at: Vec<u8> = self.start.layout.main.iter().map(|&p| sym(p) as u8).collect();
            let (mut a, mut b) = (at.clone(), syms.clone());
            a.sort_unstable();
            b.sort_unstable();
            if a == b {
                return Some(self.score(c, &at, loads));
            }
        }
        if loads.is_empty() {
            return Some(self.score(c, &self.anchor(&syms, None), loads));
        }
        if self.rules.fixed_order {
            return self.arrange_fixed(c, &syms, loads, budget);
        }
        let mut cur = self.score(c, &self.anchor(&syms, Some(loads[0])), loads);
        loop {
            let mut better = false;
            for i in 0..self.n {
                for j in i + 1..self.n {
                    if cur.syms[i] == cur.syms[j] {
                        continue;
                    }
                    if !budget.take(loads.len() as u64 * 8) {
                        return Some(cur);
                    }
                    let mut next = cur.syms.clone();
                    next.swap(i, j);
                    let s = self.score(c, &next, loads);
                    if s.score > cur.score {
                        cur = s;
                        better = true;
                    }
                }
            }
            if !better {
                return Some(cur);
            }
        }
    }

    /// The first loadout's best free placement, written onto its own slots;
    /// the colours left over go back where the item carried them, then anywhere.
    fn anchor(&self, syms: &[u8], first: Option<&Loadout>) -> Vec<u8> {
        let n = self.n;
        let mut out: Vec<Option<u8>> = vec![None; n];
        let mut taken = vec![false; n];
        if let Some(l) = first {
            let own: Vec<(usize, Card)> =
                l.main.iter().enumerate().filter_map(|(p, c)| c.map(|c| (p, c))).collect();
            let (_, to) = assign(own.len(), n, |r, col| i64::from(cost_on(own[r].1, syms[col] as usize)));
            for (r, &(p, _)) in own.iter().enumerate() {
                out[p] = Some(syms[to[r]]);
                taken[to[r]] = true;
            }
        }
        let mut left: Vec<u8> = (0..n).filter(|&k| !taken[k]).map(|k| syms[k]).collect();
        for (at, born) in out.iter_mut().zip(&self.start.layout.main) {
            if at.is_none() {
                if let Some(k) = left.iter().position(|&x| x as usize == sym(*born)) {
                    *at = Some(left.remove(k));
                }
            }
        }
        for at in out.iter_mut().filter(|x| x.is_none()) {
            *at = Some(left.remove(0));
        }
        out.into_iter().map(|x| x.expect("every slot has a colour")).collect()
    }

    fn arrange_fixed(&self, c: &Cand, syms: &[u8], loads: &[&Loadout], budget: &mut Budget) -> Option<Realized> {
        let n = self.n;
        let mut left = [0u8; NSYM];
        for &k in syms {
            left[k as usize] += 1;
        }
        // Per loadout: the room it has, and the least its remaining cards can drain.
        let room: Vec<i64> = loads
            .iter()
            .map(|l| {
                let (ex, gr) = self.extras(c, l);
                i64::from(c.capacity + gr) - i64::from(ex)
            })
            .collect();
        let lb: Vec<Vec<i64>> = loads
            .iter()
            .map(|l| {
                let mut v = vec![0i64; n + 1];
                for p in (0..n).rev() {
                    let d = l.main.get(p).copied().flatten().map_or(0, |c| i64::from(c.drain.div_ceil(2)));
                    v[p] = v[p + 1] + d;
                }
                v
            })
            .collect();
        struct St<'s, 'a, 'l> {
            space: &'s Space<'a>,
            loads: &'s [&'l Loadout],
            c: &'s Cand,
            room: Vec<i64>,
            lb: Vec<Vec<i64>>,
            used: Vec<i64>,
            cur: Vec<u8>,
            best: Option<Realized>,
        }
        fn dfs(st: &mut St, p: usize, left: &mut [u8; NSYM], budget: &mut Budget) {
            let n = st.space.n;
            if p == n {
                let r = st.space.score(st.c, &st.cur, st.loads);
                if r.score.0 >= 0 && st.best.as_ref().is_none_or(|b| r.score > b.score) {
                    st.best = Some(r);
                }
                return;
            }
            let at = |l: &Loadout, k: usize| l.main.get(p).copied().flatten().map_or(0, |c| i64::from(cost_on(c, k)));
            let born = sym(st.space.start.layout.main[p]);
            let mut order: Vec<usize> = (0..NSYM).filter(|&k| left[k] > 0).collect();
            order.sort_by_key(|&k| k != born);
            for k in order {
                if !budget.take(1) {
                    return;
                }
                let mut ok = true;
                let mut ub = i64::MAX;
                for (i, l) in st.loads.iter().enumerate() {
                    let slack = st.room[i] - (st.used[i] + at(l, k)) - st.lb[i][p + 1];
                    ok &= slack >= 0;
                    ub = ub.min(slack);
                }
                // The weakest loadout can end no better than its slack now.
                if !ok || st.best.as_ref().is_some_and(|b| ub < b.score.0) {
                    continue;
                }
                for (i, l) in st.loads.iter().enumerate() {
                    st.used[i] += at(l, k);
                }
                left[k] -= 1;
                st.cur[p] = k as u8;
                dfs(st, p + 1, left, budget);
                left[k] += 1;
                for (i, l) in st.loads.iter().enumerate() {
                    st.used[i] -= at(l, k);
                }
            }
        }
        let mut st = St { space: self, loads, c, room, lb, used: vec![0; loads.len()], cur: vec![0; n], best: None };
        dfs(&mut st, 0, &mut left, budget);
        st.best
    }

    /// The exact answer: every bucket in bill order, the relaxed score ranking
    /// which candidates are positioned first and bounding when to stop.
    fn best(&self, loads: &[&Loadout], nearest: bool, budget: &mut Budget) -> Option<Plan> {
        let mut memo = Memo::new(loads.len(), self.mains.len());
        let prepared: Vec<Prepared> = loads.iter().map(|l| Prepared::of(l)).collect();
        let relaxed = |c: &Cand, memo: &mut Memo| -> (i64, i64) {
            let mut worst = i64::MAX;
            let mut sum = 0;
            for (li, l) in loads.iter().enumerate() {
                let s = self.relaxed_spare(c, l, &prepared[li], li, memo);
                worst = worst.min(s);
                sum += s;
            }
            (worst, sum)
        };
        if nearest {
            let mut all: Vec<((i64, i64), usize)> =
                self.cands.iter().enumerate().map(|(i, c)| (relaxed(c, &mut memo), i)).collect();
            all.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| self.cands[a.1].key.cmp(&self.cands[b.1].key)));
            let mut best: Option<(Realized, usize)> = None;
            for &(_, ci) in all.iter().take(8) {
                if let Some(r) = self.realize(&self.cands[ci], loads, budget) {
                    if best.as_ref().is_none_or(|(b, _)| r.score > b.score) {
                        best = Some((r, ci));
                    }
                }
            }
            return best.map(|(r, ci)| self.plan_of(&self.cands[ci], &r, loads.len()));
        }
        let mut i = 0;
        while i < self.cands.len() {
            let key = self.cands[i].key;
            let mut j = i;
            let mut bucket: Vec<((i64, i64), usize)> = Vec::new();
            while j < self.cands.len() && self.cands[j].key == key {
                let r = relaxed(&self.cands[j], &mut memo);
                if r.0 >= 0 {
                    bucket.push((r, j));
                }
                j += 1;
            }
            bucket.sort_by_key(|x| std::cmp::Reverse(x.0));
            let mut best: Option<(Realized, usize)> = None;
            for &(r, ci) in &bucket {
                if best.as_ref().is_some_and(|(b, _)| r < (b.score.0, b.score.1)) {
                    break;
                }
                if let Some(x) = self.realize(&self.cands[ci], loads, budget) {
                    if x.score.0 >= 0 && best.as_ref().is_none_or(|(b, _)| x.score > b.score) {
                        best = Some((x, ci));
                    }
                }
                if budget.spent && best.is_some() {
                    break;
                }
            }
            if let Some((r, ci)) = best {
                let mut p = self.plan_of(&self.cands[ci], &r, loads.len());
                p.exhaustive = !budget.spent;
                return Some(p);
            }
            i = j;
        }
        None
    }

    fn plan_of(&self, c: &Cand, r: &Realized, hard: usize) -> Plan {
        Plan {
            layout: Layout {
                main: r.syms.iter().map(|&k| SYMS[k as usize]).collect(),
                exilus: self.board.exilus.and(SYMS[c.e as usize]),
                grant: self.board.grant.and(SYMS[c.g as usize]),
            },
            cost: c.cost,
            rank: c.rank,
            capacity: c.capacity,
            loadouts: r.placed[..hard.min(r.placed.len())].to_vec(),
            exhaustive: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::capacity::{fit, Investment, PlannedMod, StanceSlot};
    use Polarity::*;

    fn c(drain: u32, polarity: Polarity) -> Option<Card> {
        Some(Card { drain, polarity, ordered: false })
    }
    fn gun(main: [Option<Polarity>; 8], exilus: Option<Polarity>) -> Board {
        Board { base_max_rank: 30, main: main.to_vec(), exilus: Some(exilus), grant: None }
    }
    fn load(main: &[Option<Card>]) -> Loadout {
        Loadout { main: main.to_vec(), ..Default::default() }
    }

    /// A build that fits bare costs nothing, and the item keeps its colours.
    #[test]
    fn a_free_build_stays_free() {
        let b = gun([Some(Madurai), None, None, None, None, None, None, None], None);
        let p = plan(&b, &[load(&[c(9, Naramon), c(9, Vazarin)])], Rules::default(), None).unwrap();
        assert_eq!(p.cost.total(), 0);
        assert_eq!(p.layout.main.iter().filter(|x| x.is_some()).count(), 1);
        assert_eq!(p.loadouts[0].drain, 18);
        assert_eq!(p.loadouts[0].spare, 42);
    }

    /// TWO CONFIGS, ONE LAYOUT: each fits alone with one Forma, and together
    /// they need one of each colour, not two of either.
    #[test]
    fn a_shared_layout_serves_both_configs() {
        let b = gun([None; 8], None);
        let a = load(&[c(12, Madurai), c(12, Madurai), c(12, Madurai), c(12, Madurai),
                       c(4, Naramon), c(4, Naramon), c(4, Naramon), c(4, Naramon)]);
        let z = load(&[c(12, Vazarin), c(12, Vazarin), c(12, Vazarin), c(12, Vazarin),
                       c(4, Naramon), c(4, Naramon), c(4, Naramon), c(4, Naramon)]);
        // Alone: 64 needs one Madurai. Together: one Madurai + one Vazarin.
        let alone = plan(&b, std::slice::from_ref(&a), Rules::default(), None).unwrap();
        assert_eq!(alone.cost.total(), 1);
        let both = plan(&b, &[a, z], Rules::default(), None).unwrap();
        assert_eq!(both.cost.total(), 2);
        let m = both.layout.main.iter().filter(|&&x| x == Some(Madurai)).count();
        let v = both.layout.main.iter().filter(|&&x| x == Some(Vazarin)).count();
        assert_eq!((m, v), (1, 1));
        // The Vazarin slot holds a 4-drain mod at +25% in the first config.
        assert!(both.loadouts.iter().all(|l| l.spare == 1), "{:?}", both.loadouts);
        // The first config did not move; the second was rearranged onto it.
        assert_eq!(both.loadouts[0].slots, (0..8).map(Some).collect::<Vec<_>>());
        assert_eq!(both.loadouts[0].moved, 0);
    }

    /// Two configs whose heavy mods want different colours in slots that one
    /// cannot give both: an Omni Forma serves either, and `Allowed` takes it
    /// only because it saves a Forma.
    #[test]
    fn omni_is_bought_only_where_it_saves_a_forma() {
        let b = gun([None; 8], None);
        let a = load(&[c(14, Madurai), c(14, Madurai), c(14, Madurai), c(14, Madurai),
                       c(4, Naramon), c(4, Naramon), c(4, Naramon), c(4, Naramon)]);
        let z = load(&[c(14, Vazarin), c(14, Vazarin), c(14, Vazarin), c(14, Vazarin),
                       c(4, Naramon), c(4, Naramon), c(4, Naramon), c(4, Naramon)]);
        // 72 alone: two halved (7+7+14+14+16 = 58). Together, regular: 2 + 2.
        let reg = plan(&b, &[a.clone(), z.clone()], Rules::default(), None).unwrap();
        assert_eq!((reg.cost.regular, reg.cost.omni), (4, 0));
        let omni = Rules { omni: OmniUse::Allowed, ..Rules::default() };
        let p = plan(&b, &[a.clone(), z.clone()], omni, None).unwrap();
        assert_eq!((p.cost.regular, p.cost.omni), (0, 2));
        // …and on one config alone Omni saves nothing, so none is bought.
        let p = plan(&b, &[a], omni, None).unwrap();
        assert_eq!((p.cost.regular, p.cost.omni), (2, 0));
    }

    /// Mastery Forma are billed whether or not the layout needs them, and the
    /// planner puts them to work.
    #[test]
    fn a_rank_40_item_bills_five_and_uses_them() {
        let mut b = gun([None; 8], None);
        b.base_max_rank = 40;
        let l = load(&[c(10, Madurai); 8]);
        let p = plan(&b, std::slice::from_ref(&l), Rules::default(), None).unwrap();
        assert_eq!((p.cost.total(), p.rank, p.capacity), (5, 40, 80));
        assert_eq!(p.loadouts[0].drain, 5 * 5 + 3 * 10);
        let off = Rules { reach_max_rank: false, ..Rules::default() };
        let p = plan(&b, &[l], off, None).unwrap();
        // 80 drain: 30 + 2f capacity × 2 must cover 80 − 5f → f = 2 (68 ≥ 70? no), 3 (72 ≥ 65).
        assert_eq!(p.cost.total(), 3);
    }

    #[test]
    fn a_limit_is_named_when_it_is_the_reason() {
        let b = gun([None; 8], None);
        let l = load(&[c(14, Madurai); 8]);
        let r = Rules { forma_limit: Some(2), ..Rules::default() };
        let e = plan(&b, &[l], r, None).unwrap_err();
        assert!(matches!(e, PlanError::OverLimit { limit: 2, need } if need > 2), "{e:?}");
    }

    #[test]
    fn umbra_is_the_last_thing_bought() {
        let b = Board {
            base_max_rank: 30,
            main: vec![None; 8],
            exilus: Some(None),
            grant: Some(GrantSlot { innate: Some(Madurai), in_pool: true, fixed: false }),
        };
        let mut main = vec![c(16, Umbra), c(16, Umbra)];
        main.extend([c(12, Madurai); 6]);
        let l = Loadout { main, exilus: None, grant: Some(Card { drain: 7, polarity: Madurai, ordered: false }) };
        // 104 drain against 60 + 14.
        let p = plan(&b, std::slice::from_ref(&l), Rules::default(), None).unwrap();
        assert_eq!(p.cost.umbra, 0, "{p:?}");
        let never = Rules { umbra: UmbraUse::Never, ..Rules::default() };
        assert_eq!(plan(&b, std::slice::from_ref(&l), never, None).unwrap().cost.umbra, 0);
        // Heavier, so that only Umbra Forma can make it fit.
        let mut main = vec![c(16, Umbra), c(16, Umbra), c(16, Umbra)];
        main.extend([c(12, Madurai); 5]);
        let l = Loadout { main, ..l };
        let p = plan(&b, std::slice::from_ref(&l), Rules::default(), None).unwrap();
        assert!(p.cost.umbra > 0);
        let e = plan(&b, &[l], never, None).unwrap_err();
        assert_eq!(e, PlanError::DoesNotFit { loadouts: vec![0], umbra_off: true });
    }

    /// A build no layout fits still gets its nearest miss: everything matched.
    #[test]
    fn the_nearest_miss_polarizes_everything_it_can() {
        let b = gun([None; 8], None);
        let l = load(&[c(16, Madurai); 8]);
        assert!(plan(&b, std::slice::from_ref(&l), Rules::default(), None).is_err());
        let p = closest(&b, &[l], Rules::default(), None).unwrap();
        assert_eq!(p.layout.main, vec![Some(Madurai); 8]);
        assert_eq!(p.loadouts[0].spare, 60 - 64);
    }


    fn e(drain: u32, polarity: Polarity) -> Option<Card> {
        Some(Card { drain, polarity, ordered: true })
    }

    /// THE GREEDY IS THE ASSIGNMENT. `free_drain` stands in for the Hungarian
    /// method wherever thousands of builds are read, so on random cards and
    /// random slot multisets the two must agree to the point.
    #[test]
    fn free_drain_is_the_optimal_assignment() {
        let mut rng = 0x2545_f491_u64;
        let mut next = |m: u64| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng % m
        };
        let pols = [Polarity::Madurai, Polarity::Naramon, Polarity::Vazarin, Polarity::Umbra];
        for _ in 0..200_000 {
            let n = 1 + next(9) as usize;
            let k = next(n as u64 + 1) as usize;
            let cards: Vec<Card> = (0..k)
                .map(|_| Card { drain: 1 + next(20) as u32, polarity: pols[next(4) as usize], ordered: false })
                .collect();
            let syms: Vec<u8> = (0..n).map(|_| [BLANK, 1, 2, 3, UMBRA, AURA, OMNI][next(7) as usize] as u8).collect();
            let (want, _) = assign(k, n, |r, c| i64::from(cost_on(cards[r], syms[c] as usize)));
            let got = free_drain(&Prepared::of(&Loadout { main: cards.iter().map(|&c| Some(c)).collect(), ..Default::default() }).sorted, &syms);
            assert_eq!(i64::from(got), want, "{cards:?} on {syms:?}");
        }
    }

    /// AN ELEMENT PAIRING SURVIVES THE MOVE. The first config puts Madurai
    /// before Naramon; the second's Naramon element comes before its Madurai
    /// one and both must sit on their colour to fit. Swapping the two mods
    /// would be free and reorder the elements, so the colours move instead.
    #[test]
    fn moved_element_mods_keep_their_order() {
        let b = gun([None; 8], None);
        let fill = [c(13, Vazarin); 6];
        let mut first = vec![c(15, Madurai), c(15, Naramon)];
        first.extend(fill);
        let mut second = vec![e(15, Naramon), e(15, Madurai)];
        second.extend(fill);
        // All matched: 8 + 8 + 6 x 7 = 58. One of the two left bare: 65.
        let r = Rules { reach_max_rank: false, ..Rules::default() };
        let p = plan(&b, &[load(&first), load(&second)], r, None).unwrap();
        assert!(p.loadouts.iter().all(|l| l.spare >= 0), "{p:?}");
        let slots = &p.loadouts[1].slots;
        let at = |from: usize| slots.iter().position(|&x| x == Some(from)).unwrap();
        assert!(at(0) < at(1), "{slots:?} on {:?}", p.layout.main);
    }

    /// FIXED ORDER: nothing moves, so a slot serves whatever every config keeps
    /// there — two configs that disagree at slot 0 take an Omni Forma there.
    #[test]
    fn a_fixed_order_serves_each_slot_as_it_stands() {
        let b = gun([None; 8], None);
        let a = load(&[c(18, Madurai), c(18, Naramon), c(10, Naramon), c(10, Naramon),
                       c(4, Naramon), c(4, Naramon), c(4, Naramon), c(4, Naramon)]);
        let z = load(&[c(18, Naramon), c(18, Madurai), c(10, Naramon), c(10, Naramon),
                       c(4, Naramon), c(4, Naramon), c(4, Naramon), c(4, Naramon)]);
        // 72 each, 12 to save: an 18 halved and a 10 halved, and nothing less.
        let free = Rules { reach_max_rank: false, ..Rules::default() };
        let moved = plan(&b, &[a.clone(), z.clone()], free, None).unwrap();
        assert_eq!(moved.cost.total(), 2, "moving mods, one Madurai and one Naramon serve both");
        let fixed = Rules { fixed_order: true, omni: OmniUse::Allowed, ..free };
        let p = plan(&b, &[a.clone(), z.clone()], fixed, None).unwrap();
        assert!(p.loadouts.iter().all(|l| l.moved == 0 && l.slots[..8].iter().enumerate().all(|(i, &s)| s == Some(i))));
        assert!(p.loadouts.iter().all(|l| l.spare >= 0), "{p:?}");
        assert_eq!(p.cost.total(), 2);
        assert!(p.cost.omni > 0, "the two first slots disagree: {:?}", p.layout.main);
        // Without Omni the fixed answer has to buy more.
        let p = plan(&b, &[a, z], Rules { omni: OmniUse::Never, ..fixed }, None).unwrap();
        assert!(p.cost.total() > 2, "{:?}", p.cost);
    }

    /// THE PLAN'S OPTIMIZER: each point is the cheapest layout for its worst
    /// group, and the curve climbs.
    #[test]
    fn coverage_climbs_with_forma() {
        let b = gun([None; 8], None);
        let heavy = |p: Polarity| GroupBuild {
            loadout: load(&[c(16, p), c(16, p), c(16, p), c(16, p), c(4, Naramon), c(4, Naramon), None, None]),
            ratio: 1.0,
        };
        let light = GroupBuild { loadout: load(&[c(10, Madurai), c(10, Vazarin), c(10, Naramon)]), ratio: 0.5 };
        let groups = vec![
            Group { builds: vec![heavy(Madurai), light.clone()] },
            Group { builds: vec![heavy(Vazarin), light.clone()] },
        ];
        let r = Rules { reach_max_rank: false, ..Rules::default() };
        let cov = optimize(&b, &[], &groups, 0.0, r, None).unwrap();
        assert!(cov.exhaustive);
        let pts: Vec<(u32, f64)> = cov.curve.iter().map(|p| (p.plan.cost.total(), p.worst)).collect();
        // EVERY POINT COSTS MORE AND REACHES FURTHER THAN THE LAST. Asserted
        // here, on a fixture, because the LENGTH of a real curve is a property
        // of the board — it stops at the ceiling, which the scoring bot moves.
        assert!(pts.len() > 1, "{pts:?}");
        assert!(pts.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 > w[0].1), "{pts:?}");
        // Free: only the light build fits. 72 heavy needs two halvings each: 2 + 2.
        assert_eq!(pts.first(), Some(&(0, 0.5)), "{pts:?}");
        assert_eq!(pts.last().map(|p| p.1), Some(1.0), "{pts:?}");
        assert_eq!(pts.last().map(|p| p.0), Some(4), "{pts:?}");
        let top = cov.curve.last().unwrap();
        assert!(top.picks.iter().all(|p| p.as_ref().is_some_and(|p| p.build == 0 && p.placed.spare >= 0)));
    }

    /// A PARTIAL START: what the item already carries is free, and the Forma
    /// already spent count toward its rank.
    #[test]
    fn an_item_already_polarized_pays_only_the_difference() {
        let mut b = gun([None; 8], None);
        b.base_max_rank = 40;
        let mut layout = b.innate();
        layout.main[0] = Some(Madurai);
        layout.main[1] = Some(Madurai);
        let mut start = Start { layout, forma_spent: 5, pinned: false };
        start.layout.main.swap(1, 5);
        let l = load(&[c(10, Madurai); 8]);
        let p = plan(&b, &[l], Rules::default(), Some(&start)).unwrap();
        assert_eq!((p.cost.total(), p.capacity), (0, 80));
        // PINNED, the slots stay exactly where the start has them — here on
        // two slots a free placement would leave bare, with the mods moved on.
        let mut light = vec![None, None];
        light.extend([c(10, Madurai); 6]);
        let loose = plan(&b, &[load(&light)], Rules::default(), Some(&start)).unwrap();
        assert_ne!(loose.layout.main, start.layout.main);
        start.pinned = true;
        let p = plan(&b, &[load(&light)], Rules::default(), Some(&start)).unwrap();
        assert_eq!(p.layout.main, start.layout.main);
        assert_eq!(p.loadouts[0].spare, 80 - 5 - 5 - 40);
    }

    /// THE EXISTING PLANNER, AGREED WITH. `rules::capacity::fit` is greedy and serves the
    /// optimizer's hot path; on one loadout both must bill the same Forma, and
    /// this one never leaves less room.
    #[test]
    fn one_loadout_bills_what_fit_bills() {
        let pols = [Madurai, Naramon, Vazarin, Umbra];
        let mut rng = 0x9e37_79b9_u64;
        let mut next = |m: u64| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng % m
        };
        let mut checked = 0;
        for _ in 0..3000 {
            let max_rank = if next(3) == 0 { 40 } else { 30 };
            let innate: Vec<Option<Polarity>> = (0..9)
                .map(|_| if next(3) == 0 { Some(pols[next(3) as usize]) } else { None })
                .collect();
            let count = 1 + next(9) as usize;
            let cards: Vec<Card> = (0..count)
                .map(|_| Card { drain: 2 + next(15) as u32, polarity: pols[next(4) as usize], ordered: false })
                .collect();
            let stance = (next(3) == 0).then(|| (pols[next(3) as usize], [None, Some(Madurai), Some(Naramon)][next(3) as usize]));
            let inv = Investment { use_umbra: false, ..Investment::default() };
            let planned: Vec<PlannedMod> =
                cards.iter().map(|c| PlannedMod { base_drain: c.drain, polarity: c.polarity }).collect();
            let old = fit(
                max_rank,
                &innate,
                &planned,
                inv,
                stance.map(|(m, s)| StanceSlot { mod_polarity: m, slot_polarity: s }),
            );
            let board = Board {
                base_max_rank: max_rank,
                main: innate[..8].to_vec(),
                exilus: Some(innate[8]),
                grant: stance.map(|(_, s)| GrantSlot { innate: s, in_pool: false, fixed: false }),
            };
            let mut main: Vec<Option<Card>> = cards.iter().take(8).map(|&c| Some(c)).collect();
            main.resize(8, None);
            let l = Loadout {
                main,
                exilus: cards.get(8).copied(),
                grant: stance.map(|(m, _)| Card { drain: 5, polarity: m, ordered: false }),
            };
            let new = plan(&board, &[l], Rules::default(), None);
            match (old, new) {
                (Ok(o), Ok(n)) => {
                    let key = |c: FormaCost| (c.umbra, c.total());
                    assert!(
                        key(n.cost) <= key(o.cost),
                        "fit {:?} vs plan {:?} for {cards:?} on {innate:?} stance {stance:?}",
                        o.cost, n.cost
                    );
                    if key(n.cost) == key(o.cost) {
                        assert!(
                            n.loadouts[0].spare >= (o.capacity - o.drain) as i32,
                            "less room than fit: {o:?} vs {n:?} for {cards:?} on {innate:?}"
                        );
                    }
                    checked += 1;
                }
                (Err(_), Err(_)) => {}
                (o, n) => panic!("disagree on legality: fit {o:?}, plan {n:?} for {cards:?} on {innate:?}"),
            }
        }
        assert!(checked > 1000, "{checked}");
    }
}

