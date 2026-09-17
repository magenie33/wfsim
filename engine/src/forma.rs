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

use crate::mods::{capacity, forma_to_max_rank, rank_after, slot_drain, FormaCost, Polarity};

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
        use crate::weapons_data as w;
        let spec = w::spec(id)?;
        Some(Self {
            base_max_rank: spec.max_rank,
            main: w::innate_slots(id).to_vec(),
            exilus: w::has_exilus_slot(id).then(|| w::exilus_polarity(id)),
            grant: (spec.slot == "melee").then(|| GrantSlot {
                innate: w::stance_polarity(id),
                in_pool: false,
            }),
        })
    }

    /// A Warframe: rank 30, eight main slots, an exilus and an aura slot whose
    /// polarities are one pool with the rest.
    pub fn warframe(frame: &crate::warframes_data::WarframeDef) -> Self {
        use crate::weapons_data::polarity;
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
}

/// One loadout placed on the planned layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// Per main slot, the index into the loadout's `main` of the mod put there.
    /// The first loadout is never moved: `slots[i] == Some(i)` for its mods.
    pub slots: Vec<Option<usize>>,
    pub drain: u32,
    /// What its grant card hands back on the planned grant slot.
    pub grant: u32,
    /// `capacity + grant - drain`.
    pub spare: u32,
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
}

/// Plan one layout for every loadout. The first loadout is the one being edited
/// and keeps its mod positions; the others are rearranged onto the layout.
///
/// The order a layout is judged in: Umbra Forma (under `WhenNeeded`), then the
/// grant slot (under `grant_slot_first`), then Forma, then Omni among equal
/// Forma, then the WORST loadout's spare capacity, then the total, then the
/// fewest of the item's own colours moved.
pub fn plan(
    board: &Board,
    loadouts: &[Loadout],
    rules: Rules,
    start: Option<&Start>,
) -> Result<Plan, String> {
    let n = board.main.len();
    if n > MAX_SLOTS {
        return Err(format!("{n} slots is more than the planner takes ({MAX_SLOTS})"));
    }
    if loadouts.is_empty() {
        return Err("no loadout to plan for".into());
    }
    for (i, l) in loadouts.iter().enumerate() {
        let mods = l.main.iter().flatten().count();
        if l.main.len() > n {
            return Err(format!("loadout {}: {} slots given, the item has {n}", i + 1, l.main.len()));
        }
        if mods > n {
            return Err(format!("loadout {}: {mods} mods for {n} slots", i + 1));
        }
        if l.exilus.is_some() && board.exilus.is_none() {
            return Err(format!("loadout {}: the item has no exilus slot", i + 1));
        }
        if l.grant.is_some() && board.grant.is_none() {
            return Err(format!("loadout {}: the item has no stance or aura slot", i + 1));
        }
    }
    let innate = Start { layout: board.innate(), forma_spent: 0 };
    let start = start.unwrap_or(&innate);
    if start.layout.main.len() != n {
        return Err(format!("the current layout has {} main slots, the item has {n}", start.layout.main.len()));
    }
    if let Some(p) = search(board, loadouts, rules, start) {
        return Ok(p);
    }

    // WHY NOT — the one case the page has to push back, so it names the cause.
    if let Some(limit) = rules.forma_limit {
        if let Some(p) = search(board, loadouts, Rules { forma_limit: None, ..rules }, start) {
            return Err(format!("needs {} Forma, over the limit of {limit}", p.cost.total()));
        }
    }
    let alone: Vec<usize> = (0..loadouts.len())
        .filter(|&i| search(board, &loadouts[i..=i], rules, start).is_none())
        .map(|i| i + 1)
        .collect();
    let umbra_note = if rules.umbra == UmbraUse::Never
        && loadouts.iter().any(|l| l.main.iter().flatten().chain(&l.exilus).any(|c| c.polarity == Polarity::Umbra))
    {
        " — an Umbra mod pays full drain while Umbra Forma is off"
    } else {
        ""
    };
    Err(match alone.as_slice() {
        [] => "these loadouts cannot share one polarity layout".into(),
        _ if loadouts.len() == 1 => format!("does not fit even fully polarized{umbra_note}"),
        many => format!(
            "loadout {} does not fit even fully polarized{umbra_note}",
            many.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
        ),
    })
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

/// The fewest-drain placement of `mods` on the slots `syms`.
fn main_drain(mods: &[Card], syms: &[usize]) -> (u32, [usize; MAX_SLOTS]) {
    let (total, at) = assign(mods.len(), syms.len(), |r, c| {
        i64::from(slot_drain(mods[r].drain, mods[r].polarity, SYMS[syms[c]]))
    });
    (total as u32, at)
}

struct Billed {
    cost: FormaCost,
    rank: u32,
    capacity: u32,
    key: [u32; 4],
}

fn search(board: &Board, loadouts: &[Loadout], rules: Rules, start: &Start) -> Option<Plan> {
    let n = board.main.len();
    let grant_in_pool = board.grant.is_some_and(|g| g.in_pool);
    let s_grant = sym(start.layout.grant);

    // THE START AS A POOL. A bought polarization is one slot of the target
    // multiset the start does not cover — blank counted as a colour of its own,
    // since blanking a slot takes a Forma too — so the bill is Σ max(0, T − S).
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

    let mods: Vec<Vec<Card>> = loadouts.iter().map(|l| l.main.iter().flatten().copied().collect()).collect();

    // THE ALPHABET: a bare slot, what the item already carries, and what some
    // card can match. A colour nobody carries only ever mismatches.
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

    // Every multiset of `n` over the alphabet, as sorted symbol lists.
    let mut mains: Vec<[u8; MAX_SLOTS]> = Vec::new();
    let mut cur = [0u8; MAX_SLOTS];
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
    walk(&alpha, 0, 0, n, &mut cur, &mut mains);

    let floor = if rules.reach_max_rank { forma_to_max_rank(board.base_max_rank) } else { 0 };
    let bill = |t: &Counts, g: usize| -> Option<Billed> {
        let mut ops = [0i32; NSYM];
        for k in 0..NSYM {
            ops[k] = (t[k] - s[k]).max(0);
        }
        if board.grant.is_some() && !grant_in_pool && g != s_grant {
            ops[g] += 1;
        }
        // No Forma makes the Aura colour; only an item born with it has one.
        if ops[AURA] > 0 {
            return None;
        }
        if rules.umbra == UmbraUse::Never && ops[UMBRA] > 0 {
            return None;
        }
        if rules.omni == OmniUse::Never && ops[OMNI] > 0 {
            return None;
        }
        if rules.omni == OmniUse::Preferred && REGULAR.clone().any(|k| ops[k] > 0) {
            return None;
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
            return None;
        }
        let rank = rank_after(board.base_max_rank.max(30), spent + extra);
        let grant_miss = if rules.grant_slot_first && cost.total() > 0 {
            loadouts
                .iter()
                .filter_map(|l| l.grant)
                .filter(|&c| !grant_matched(c, SYMS[g]))
                .count() as u32
        } else {
            0
        };
        let key = match rules.umbra {
            UmbraUse::Allowed => [grant_miss, cost.total(), cost.umbra, cost.omni],
            _ => [cost.umbra, grant_miss, cost.total(), cost.omni],
        };
        Some(Billed { cost, rank, capacity: capacity(rank, rules.catalyst), key })
    };

    let mut cands: Vec<([u32; 4], u32, u16, u16)> = Vec::new();
    for (mi, m) in mains.iter().enumerate() {
        let mut t: Counts = [0; NSYM];
        for &k in &m[..n] {
            t[k as usize] += 1;
        }
        for (ei, &e) in ex_alpha.iter().enumerate() {
            let mut te = t;
            if board.exilus.is_some() {
                te[e] += 1;
            }
            for (gi, &g) in g_alpha.iter().enumerate() {
                let mut tg = te;
                if grant_in_pool {
                    tg[g] += 1;
                }
                if let Some(b) = bill(&tg, g) {
                    cands.push((b.key, mi as u32, ei as u16, gi as u16));
                }
            }
        }
    }
    cands.sort_unstable();

    const UNSET: u32 = u32::MAX;
    let mut memo = vec![vec![UNSET; mains.len()]; loadouts.len()];
    let s_exilus = sym(start.layout.exilus);
    let mut best: Option<((i64, i64, i32), usize)> = None;
    let mut i = 0;
    while i < cands.len() {
        let key = cands[i].0;
        let mut j = i;
        while j < cands.len() && cands[j].0 == key {
            let (_, mi, ei, gi) = cands[j];
            let (mi, e, g) = (mi as usize, ex_alpha[ei as usize], g_alpha[gi as usize]);
            let mut t: Counts = [0; NSYM];
            for &k in &mains[mi][..n] {
                t[k as usize] += 1;
            }
            if board.exilus.is_some() {
                t[e] += 1;
            }
            if grant_in_pool {
                t[g] += 1;
            }
            let b = bill(&t, g).expect("a candidate was billable");
            let (mut worst, mut sum) = (i64::MAX, 0i64);
            for (li, l) in loadouts.iter().enumerate() {
                if memo[li][mi] == UNSET {
                    let syms: Vec<usize> = mains[mi][..n].iter().map(|&k| k as usize).collect();
                    memo[li][mi] = main_drain(&mods[li], &syms).0;
                }
                let drain = memo[li][mi]
                    + l.exilus.map_or(0, |c| slot_drain(c.drain, c.polarity, SYMS[e]));
                let grant = l.grant.map_or(0, |c| grant_on(c, SYMS[g]));
                let spare = i64::from(b.capacity) + i64::from(grant) - i64::from(drain);
                worst = worst.min(spare);
                sum += spare;
                if spare < 0 {
                    break;
                }
            }
            // …and among equals, the one that moves the item's own colours least.
            let kept = -i32::from(e != s_exilus) - i32::from(g != s_grant);
            if worst >= 0 && best.is_none_or(|(s, _)| (worst, sum, kept) > s) {
                best = Some(((worst, sum, kept), j));
            }
            j += 1;
        }
        if best.is_some() {
            break;
        }
        i = j;
    }
    let (_, at) = best?;
    let (_, mi, ei, gi) = cands[at];
    let (e, g) = (ex_alpha[ei as usize], g_alpha[gi as usize]);
    let syms: Vec<usize> = mains[mi as usize][..n].iter().map(|&k| k as usize).collect();
    let mut t: Counts = [0; NSYM];
    for &k in &syms {
        t[k] += 1;
    }
    if board.exilus.is_some() {
        t[e] += 1;
    }
    if grant_in_pool {
        t[g] += 1;
    }
    let billed = bill(&t, g).expect("the winner was billable");

    // POSITIONS. The first loadout's mods stay where they are and take the
    // colours its best placement gave them; a colour left over goes back to
    // the empty slot it was born on where it can, then to any empty slot.
    let first = &loadouts[0];
    let firsts: Vec<(usize, Card)> =
        first.main.iter().enumerate().filter_map(|(p, c)| c.map(|c| (p, c))).collect();
    let (_, to) = main_drain(&mods[0], &syms);
    let mut slot: Vec<Option<usize>> = vec![None; n];
    let mut taken = vec![false; n];
    for (r, &(p, _)) in firsts.iter().enumerate() {
        slot[p] = Some(syms[to[r]]);
        taken[to[r]] = true;
    }
    let mut left: Vec<usize> = (0..n).filter(|&k| !taken[k]).map(|k| syms[k]).collect();
    for (at, born) in slot.iter_mut().zip(&start.layout.main) {
        if at.is_none() {
            if let Some(k) = left.iter().position(|&x| x == sym(*born)) {
                *at = Some(left.remove(k));
            }
        }
    }
    for at in slot.iter_mut().filter(|x| x.is_none()) {
        *at = Some(left.remove(0));
    }
    let slot: Vec<usize> = slot.into_iter().map(|x| x.expect("every slot has a colour")).collect();

    let placed = loadouts
        .iter()
        .enumerate()
        .map(|(li, l)| {
            let mut slots = vec![None; n];
            let own: Vec<(usize, Card)> =
                l.main.iter().enumerate().filter_map(|(p, c)| c.map(|c| (p, c))).collect();
            let main = if li == 0 {
                for &(p, _) in &own {
                    slots[p] = Some(p);
                }
                own.iter().map(|&(p, c)| slot_drain(c.drain, c.polarity, SYMS[slot[p]])).sum()
            } else {
                let cards: Vec<Card> = own.iter().map(|&(_, c)| c).collect();
                let (d, at) = main_drain(&cards, &slot);
                for (r, &(p, _)) in own.iter().enumerate() {
                    slots[at[r]] = Some(p);
                }
                d
            };
            let drain = main + l.exilus.map_or(0, |c| slot_drain(c.drain, c.polarity, SYMS[e]));
            let grant = l.grant.map_or(0, |c| grant_on(c, SYMS[g]));
            Placed { slots, drain, grant, spare: billed.capacity + grant - drain }
        })
        .collect();

    Some(Plan {
        layout: Layout {
            main: slot.iter().map(|&k| SYMS[k]).collect(),
            exilus: board.exilus.and(SYMS[e]),
            grant: board.grant.and(SYMS[g]),
        },
        cost: billed.cost,
        rank: billed.rank,
        capacity: billed.capacity,
        loadouts: placed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{fit, Investment, PlannedMod, StanceSlot};
    use Polarity::*;

    fn c(drain: u32, polarity: Polarity) -> Option<Card> {
        Some(Card { drain, polarity })
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
        assert!(e.contains("over the limit of 2"), "{e}");
    }

    #[test]
    fn umbra_is_the_last_thing_bought() {
        let b = Board {
            base_max_rank: 30,
            main: vec![None; 8],
            exilus: Some(None),
            grant: Some(GrantSlot { innate: Some(Madurai), in_pool: true }),
        };
        let mut main = vec![c(16, Umbra), c(16, Umbra)];
        main.extend([c(12, Madurai); 6]);
        let l = Loadout { main, exilus: None, grant: Some(Card { drain: 7, polarity: Madurai }) };
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
        assert!(e.contains("Umbra Forma is off"), "{e}");
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
        let start = Start { layout, forma_spent: 5 };
        let l = load(&[c(10, Madurai); 8]);
        let p = plan(&b, &[l], Rules::default(), Some(&start)).unwrap();
        assert_eq!((p.cost.total(), p.capacity), (0, 80));
    }

    /// THE EXISTING PLANNER, AGREED WITH. `mods::fit` is greedy and serves the
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
                .map(|_| Card { drain: 2 + next(15) as u32, polarity: pols[next(4) as usize] })
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
                grant: stance.map(|(_, s)| GrantSlot { innate: s, in_pool: false }),
            };
            let mut main: Vec<Option<Card>> = cards.iter().take(8).map(|&c| Some(c)).collect();
            main.resize(8, None);
            let l = Loadout {
                main,
                exilus: cards.get(8).copied(),
                grant: stance.map(|(m, _)| Card { drain: 5, polarity: m }),
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
                            n.loadouts[0].spare >= o.capacity - o.drain,
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

