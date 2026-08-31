//! Mod capacity & polarity math — the gate in front of mod resolution
//! (pipeline layer [1]). Source: wiki `Polarity` (docs/MECHANICS.md §2).
//!
//! - Matching slot polarity: drain **−50%, rounded UP** (11 → 6).
//! - Mismatched polarity: drain **+25%, rounded half-UP** (11 → 13.75 → 14;
//!   MEASURED 2026-07-24, user: 10 → 12.5 → **13**).
//! - Unpolarized slot: full drain.
//! - Capacity = weapon rank (max 30), doubled by an Orokin Catalyst → 60.
//!   (Aura/Stance capacity-bonus polarities are a separate rule, not yet
//!   needed for guns.)

/// Mod/slot polarity (wiki `Polarity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Madurai,
    Naramon,
    Vazarin,
    Zenurik,
    Unairu,
    Penjaga,
    Umbra,
    /// AURA'S polarity on a WEAPON slot — a slot polarity only, like Omni, and
    /// unlike Omni it matches NOTHING: no weapon mod carries it, so a mod put
    /// there always pays the +25% mismatch. `slot_drain`'s ordinary
    /// `Some(_)` arm is already that rule.
    ///
    /// Weapons really do ship one. The Vinquibus is *"Innate one Madurai and
    /// one Aura polarities"* (its page), and this roster had recorded only the
    /// Madurai — which reads as one polarised slot where the truth is two, one
    /// of them a penalty.
    Aura,
    /// OMNI FORMA's universal polarity — a SLOT polarity only; no mod has it.
    /// "Matches any mod except Umbra mods" (wiki `Omni Forma`), which is why
    /// [`slot_drain`] tests the mod's polarity rather than just comparing.
    ///
    /// It existed in the UI's polarity list and NOT in this enum, which was
    /// harmless only because user-chosen polarities are never sent to the
    /// engine — the client did its own capacity arithmetic. It stops being
    /// harmless the moment the planner is asked to spend Omni Forma
    /// (docs/INVESTMENT.md).
    Omni,
}

/// Effective drain of a mod in a slot.
/// WHAT A STANCE HANDS BACK, at max rank.
///
/// A stance is an AURA, not a cost: *"Similar to Aura mods, Stances can be
/// slotted into a special Stance slot on melee weapons, and they increase a
/// weapon's mod capacity"*, and *"All Stances provide a bonus mod capacity of 5
/// when maxed, doubling it to 10 when placed on the matching polarity"* (wiki,
/// Stance). ONE NUMBER FOR EVERY STANCE, which is why it is a constant here and
/// not a field on each card.
///
/// FORMA ON THE STANCE SLOT IS NOT PLANNED. *"As with Aura slots, Stance slots
/// can be repolarized using Forma"* — so a build that would buy the double by
/// polarizing reads five capacity LOW here, which is the conservative
/// direction: a build that fits here fits in game.
pub const STANCE_CAPACITY_GRANT: u32 = 5;

/// The capacity a stance adds, given the slot it sits in — and there are THREE
/// answers, not two.
///
/// The Aura page states the arithmetic a stance shares: *"equipping an Aura
/// with a matching polarity increases mod capacity … by DOUBLE of the Aura's
/// 'drain' parameter … In a slot WITHOUT a polarity, the capacity is the same
/// as the listed drain, and in a slot of a DIFFERENT polarity, the additional
/// capacity is 80% of listed drain, ROUNDED DOWN (e.g. a drain of 5 generates a
/// capacity of 4)"*.
///
/// So a wrong colour is a PENALTY and not merely the undoubled grant: 10 on a
/// match, 5 on a bare slot, 4 on a mismatch. It is the grant's mirror of the
/// +25% a mismatched slot charges a MOD.
pub fn stance_capacity(mod_polarity: Polarity, slot_polarity: Option<Polarity>) -> u32 {
    match slot_polarity {
        // Omni is universal, so it matches whatever is in the slot.
        Some(p) if p == mod_polarity || p == Polarity::Omni => STANCE_CAPACITY_GRANT * 2,
        Some(_) => (f64::from(STANCE_CAPACITY_GRANT) * 0.8).floor() as u32,
        None => STANCE_CAPACITY_GRANT,
    }
}

/// THE STANCE SLOT AS THE PLANNER SEES IT: what is in it, and what colour it is
/// before any Forma.
///
/// It is a slot the planner can POLARIZE like any other, and the one whose
/// polarization buys capacity outright instead of halving a drain — five
/// points, which beats polarizing any mod draining ten or less. That is why it
/// cannot be settled before planning: which is worth more is what the planner
/// is for.
#[derive(Debug, Clone, Copy)]
pub struct StanceSlot {
    pub mod_polarity: Polarity,
    pub slot_polarity: Option<Polarity>,
}

impl StanceSlot {
    /// Does the slot already carry the stance's colour?
    fn matched(self) -> bool {
        self.slot_polarity == Some(self.mod_polarity)
    }
}

pub fn slot_drain(base_drain: u32, mod_polarity: Polarity, slot_polarity: Option<Polarity>) -> u32 {
    match slot_polarity {
        // Omni matches ANY mod except an Umbra one, where it is simply a
        // mismatch like any other colour.
        Some(Polarity::Omni) if mod_polarity != Polarity::Umbra => base_drain.div_ceil(2),
        Some(p) if p == mod_polarity => base_drain.div_ceil(2), // −50%, round up
        Some(_) => {
            // +25%, rounded half-up (user-measured: 10 -> 13). f64::round
            // rounds half away from zero, which matches.
            ((base_drain as f64) * 1.25).round() as u32
        }
        None => base_drain,
    }
}

/// Weapon mod capacity: rank, doubled by an Orokin Catalyst/Reactor.
///
/// "Items have a limited Mod Capacity, that correlates to their Rank" and a
/// Catalyst "doubles the available Mod capacity" (wiki `Mod Capacity`).
pub fn capacity(rank: u32, catalyst: bool) -> u32 {
    if catalyst {
        rank * 2
    } else {
        rank
    }
}

/// A weapon's max rank AFTER `forma` polarizations.
///
/// Every weapon starts at 30. A rank-40 weapon (Kuva/Tenet/Coda, Paracesis)
/// gains TWO max rank per Forma and caps out after five: "max rank caps at 40
/// after 5 polarizations (max rank increases by 2 per Forma added)" (wiki
/// `Paracesis`). The `.min` says both cases at once — on a rank-30 weapon
/// Forma adds nothing at all.
pub fn rank_after(base_max_rank: u32, forma: u32) -> u32 {
    debug_assert!(base_max_rank >= 30, "a weapon starts at 30");
    (30 + 2 * forma).min(base_max_rank)
}

/// Polarizations needed to reach a weapon's own ceiling. FIVE for a rank-40
/// weapon, none for anyone else.
///
/// Worth its own name because it is a MASTERY figure, not a capacity one: a
/// build may fit in three, and the fifth is still what full affinity requires. It is why `polarize_to_max` is the default and why the
/// default path never has to solve for its own capacity.
pub fn forma_to_max_rank(base_max_rank: u32) -> u32 {
    base_max_rank.saturating_sub(30).div_ceil(2)
}

/// The three choices that belong to the PLAYER rather than to the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Investment {
    /// Doubles capacity. On by default — a modded weapon has one.
    pub catalyst: bool,
    /// Spend the full five polarizations on a rank-40 weapon even when the
    /// build would fit in fewer, because that is what full mastery affinity
    /// takes. ON by default.
    pub polarize_to_max: bool,
    /// Omni Forma for every polarized slot: it matches any mod except an Umbra
    /// one, so it removes the colour puzzle — at a higher price.
    pub use_omni: bool,
    /// May the planner spend an UMBRA Forma?
    ///
    /// The rule is "as little as possible, but use it rather than fail", and [`fit`] implements exactly that: it plans
    /// without, and only retries with when without is impossible. So this flag
    /// is the FIRST attempt's answer, not a veto — a build that genuinely
    /// needs an Umbra Forma still gets one.
    pub use_umbra: bool,
}

impl Default for Investment {
    fn default() -> Self {
        Self {
            catalyst: true,
            polarize_to_max: true,
            use_omni: false,
            use_umbra: false,
        }
    }
}

/// What a plan costs, by Forma TYPE. Three numbers because they are three
/// different items, and a player who has Forma may still have no Umbra Forma.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FormaCost {
    pub regular: u32,
    pub omni: u32,
    pub umbra: u32,
}

impl FormaCost {
    pub fn total(&self) -> u32 {
        self.regular + self.omni + self.umbra
    }
}

/// One equipped mod placed in a slot.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub base_drain: u32,
    pub mod_polarity: Polarity,
    pub slot_polarity: Option<Polarity>,
}

/// Validate a loadout against capacity. Returns the total drain, or an
/// error naming the overflow (rigor rule: an over-capacity build is
/// impossible in-game and must be rejected, never silently accepted).
pub fn validate_loadout(cap: u32, placements: &[Placement]) -> Result<u32, String> {
    let used: u32 = placements
        .iter()
        .map(|p| slot_drain(p.base_drain, p.mod_polarity, p.slot_polarity))
        .sum();
    if used > cap {
        Err(format!(
            "loadout uses {used} capacity, exceeding the {cap} cap"
        ))
    } else {
        Ok(used)
    }
}

/// A mod to be fitted by the forma planner.
#[derive(Debug, Clone, Copy)]
pub struct PlannedMod {
    pub base_drain: u32,
    pub polarity: Polarity,
}

/// Result of forma planning.
#[derive(Debug, Clone)]
pub struct FormaPlan {
    /// Polarity on each slot after planning (index-aligned with mods; the
    /// mod at index i sits in slot i). `None` = blank slot.
    pub slots: Vec<Option<Polarity>>,
    /// Total polarizations the BUILD needs. `cost.total()`.
    pub forma_used: u32,
    /// The same figure split by item type — they are not interchangeable.
    pub cost: FormaCost,
    pub total_drain: u32,
}

/// Auto-forma: fit one mod per slot into `cap`, starting from the weapon's
/// innate polarity pool, using as few Forma as possible. Mismatches are
/// never beneficial (blanks are strictly better), so the planner only ever
/// matches or leaves blank; innate polarities can be freely rearranged
/// among slots (Forma allows repositioning), so they form a POOL.
pub fn plan_forma(
    cap: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
) -> Result<FormaPlan, String> {
    plan_forma_with(cap, innate_slots, mods, Investment::default())
}

/// The planner, told which Forma the player is willing to spend.
///
/// Two rules decide what a polarization COSTS, and both come from what the
/// polarity can match:
///
/// - an **Umbra mod** can only be matched by an Umbra Forma. No regular
///   polarity is Umbra, and Omni explicitly is not either ("matches any mod
///   except Umbra mods"). So with `use_umbra` off it is never matched, and it
///   pays full drain rather than blocking the build.
/// - anything else takes a **regular** Forma, or an **Omni** one when the
///   player has chosen Omni — the owner's rule is all-or-nothing.
pub fn plan_forma_with(
    cap: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
    inv: Investment,
) -> Result<FormaPlan, String> {
    plan_forma_spending(cap, innate_slots, mods, inv, 0)
}

/// The planner, told a MINIMUM number of polarizations it is going to spend
/// anyway.
///
/// `at_least` is mastery's figure, not the build's: five polarizations is what
/// full affinity takes on a rank-40 weapon whether or not the build needs them.
/// Spending them and then not USING them was leaving capacity on the table —
/// the same five Forma, placed on the five biggest mods instead of the two the
/// build strictly needed, leave more room for whatever comes next..
fn plan_forma_spending(
    cap: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
    inv: Investment,
    at_least: u32,
) -> Result<FormaPlan, String> {
    assert!(mods.len() <= innate_slots.len(), "more mods than slots");
    let mut matched = vec![false; mods.len()];
    let mut cost = FormaCost::default();
    // What polarity a Forma'd slot ends up carrying, which is what the plan
    // reports back and what the UI draws.
    let placed = |m: &PlannedMod| {
        if inv.use_omni && m.polarity != Polarity::Umbra {
            Polarity::Omni
        } else {
            m.polarity
        }
    };

    // Biggest-drain mods first for every greedy choice.
    let mut order: Vec<usize> = (0..mods.len()).collect();
    order.sort_by(|&a, &b| mods[b].base_drain.cmp(&mods[a].base_drain));

    // 1. Spend the innate polarity pool on the biggest matching mods.
    let mut pool: Vec<Polarity> = innate_slots.iter().flatten().copied().collect();
    for &i in &order {
        if let Some(pos) = pool.iter().position(|&p| p == mods[i].polarity) {
            pool.remove(pos);
            matched[i] = true;
        }
    }

    let drain = |matched: &[bool]| -> u32 {
        mods.iter()
            .zip(matched)
            .map(|(m, &ok)| {
                if ok {
                    m.base_drain.div_ceil(2)
                } else {
                    m.base_drain
                }
            })
            .sum()
    };

    // 2. Forma the biggest unmatched mod until the build fits. A mod the
    //    player will not buy a Forma for is skipped, not refused — it simply
    //    keeps paying full drain.
    let affordable = |m: &PlannedMod| m.polarity != Polarity::Umbra || inv.use_umbra;
    while drain(&matched) > cap {
        let Some(&next) = order
            .iter()
            .find(|&&i| !matched[i] && affordable(&mods[i]))
        else {
            let umbra_blocked = !inv.use_umbra
                && order
                    .iter()
                    .any(|&i| !matched[i] && mods[i].polarity == Polarity::Umbra);
            return Err(format!(
                "build needs {} capacity even fully forma'd (cap {cap}){}",
                drain(&matched),
                if umbra_blocked {
                    " — an Umbra mod is unmatched and Umbra Forma is switched off"
                } else {
                    ""
                }
            ));
        };
        matched[next] = true;
        if mods[next].polarity == Polarity::Umbra {
            cost.umbra += 1;
        } else if inv.use_omni {
            cost.omni += 1;
        } else {
            cost.regular += 1;
        }
    }

    // 3. Polarizations that are being spent anyway go to work. Biggest first,
    //    like every other choice here, so the room they buy is the most the
    //    same Forma could have bought.
    while cost.total() < at_least {
        let Some(&next) = order
            .iter()
            .find(|&&i| !matched[i] && affordable(&mods[i]))
        else {
            break;
        };
        matched[next] = true;
        if mods[next].polarity == Polarity::Umbra {
            cost.umbra += 1;
        } else if inv.use_omni {
            cost.omni += 1;
        } else {
            cost.regular += 1;
        }
    }

    let slots = mods
        .iter()
        .zip(&matched)
        .map(|(m, &ok)| if ok { Some(placed(m)) } else { None })
        .collect();
    Ok(FormaPlan {
        slots,
        forma_used: cost.total(),
        cost,
        total_drain: drain(&matched),
    })
}

/// What the layout a player has ACTUALLY SET costs — a different question from
/// [`fit`], which answers what the cheapest layout would be.
///
/// Both are real and the UI asks both: one is "what does this build cost me",
/// the other is "what should I do". The rule for the bill is that innate
/// polarities form a free pool a Forma may REPOSITION, so an added slot and a
/// removed one cancel: `max(added, removed)`. Blanking an innate polarity costs
/// a Forma on its own, which is why the maximum and not the sum.
///
/// Umbra and Omni are counted apart because they are different items and no
/// weapon is born with either.
/// `slots` is EVERY slot, not only the filled ones. A slot with an innate
/// polarity and no mod in it still carries that polarity — passing only the
/// filled slots reads as "the rest were blanked", and blanking costs a Forma.
/// Empty slots go in with `base_drain: 0`.
pub fn cost_of(innate_slots: &[Option<Polarity>], slots: &[Placement]) -> (u32, FormaCost) {
    let drain: u32 = slots
        .iter()
        .map(|p| slot_drain(p.base_drain, p.mod_polarity, p.slot_polarity))
        .sum();

    let mut cost = FormaCost::default();
    let mut need: Vec<Polarity> = Vec::new();
    let (mut umbra_used, mut omni_used) = (0u32, 0u32);
    for p in slots.iter().filter_map(|p| p.slot_polarity) {
        match p {
            Polarity::Omni => omni_used += 1,
            Polarity::Umbra => umbra_used += 1,
            other => need.push(other),
        }
    }
    // AN INNATE UMBRA IS NOT A PURCHASE. Some weapons are born with one, and
    // keeping it costs nothing — billing for it charged the player for a slot
    // the game gave them. Only what is used BEYOND the
    // innate ones is bought. Omni is netted the same way for symmetry; no
    // weapon is born with one today, and if one ever is, this already says the
    // right thing.
    let innate_of = |want: Polarity| {
        innate_slots.iter().flatten().filter(|&&p| p == want).count() as u32
    };
    cost.umbra = umbra_used.saturating_sub(innate_of(Polarity::Umbra));
    cost.omni = omni_used.saturating_sub(innate_of(Polarity::Omni));
    let pool: Vec<Polarity> = innate_slots
        .iter()
        .flatten()
        .copied()
        .filter(|p| !matches!(p, Polarity::Omni | Polarity::Umbra))
        .collect();
    let count = |xs: &[Polarity], p: Polarity| xs.iter().filter(|&&x| x == p).count() as i64;
    let (mut added, mut removed) = (0i64, 0i64);
    let mut seen: Vec<Polarity> = need.iter().chain(pool.iter()).copied().collect();
    seen.sort_by_key(|p| format!("{p:?}"));
    seen.dedup();
    for p in seen {
        let d = count(&need, p) - count(&pool, p);
        if d > 0 {
            added += d;
        } else {
            removed -= d;
        }
    }
    cost.regular = added.max(removed) as u32;
    (drain, cost)
}

/// What a build costs to OWN: the rank it needs, the capacity that gives it,
/// and the Forma that gets there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    /// Max rank the weapon is at once the Forma below are spent.
    pub rank: u32,
    pub capacity: u32,
    pub drain: u32,
    pub cost: FormaCost,
    /// Polarity per slot, index-aligned with the mods handed in.
    pub slots: Vec<Option<Polarity>>,
}

/// THE question the builder asks: what would it take to own this build?
///
/// # The strategy, in priority order
///
/// 1. **Reach max rank first.** Five polarizations on a rank-40 weapon, because
///    that is what full mastery affinity takes. It is a floor, not a budget.
/// 2. **Then as FEW Forma as possible to make the build legal** — and Umbra
///    Forma only when refusing would be inventing a rule the game does not
///    have. A weapon born with an Umbra polarity keeps it: that slot was never
///    bought, so it is never billed and never overwritten if it can be used.
/// 3. **Then as much SPARE CAPACITY as possible.** Every polarization that is
///    being bought anyway goes on the biggest mod still unpolarized.
///
/// The order matters: 2 before 3 means the answer is never "spend one more
/// Forma to leave more room", and 1 before 2 means the room the mastery Forma
/// buy is available to 2 before it starts counting.
///
/// CAPACITY IS NOT AN INPUT on a rank-40 weapon: every Forma both polarizes a
/// slot and adds two max rank, so how much room there is depends on how much
/// Forma was spent, which depends on how much room there is. The default
/// settles it by not asking — five polarizations is what full mastery affinity
/// takes either way. With `polarize_to_max` off the loop is real and is solved
/// by SEARCHING the budget: for each f from 0 up, ask what the build needs at
/// the capacity f would buy and take the first f that covers its own answer.
pub fn fit(
    base_max_rank: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
    inv: Investment,
    // THE STANCE SLOT — see [`StanceSlot`]. `None` on every weapon that has no
    // stance slot, and on a melee build with the slot left empty.
    stance: Option<StanceSlot>,
) -> Result<Fitted, String> {
    // AS LITTLE UMBRA AS POSSIBLE, BUT NEVER FAIL FOR WANT OF IT. Umbra Forma
    // is the scarce item, so the first attempt does without — and if the build
    // is impossible without it, refusing would be inventing a rule the game
    // does not have.
    //
    // The two attempts call `fit_exactly`, not each other: a version of this
    // that recursed into `fit` re-entered the same branch and never bottomed
    // out.
    if !inv.use_umbra && mods.iter().any(|m| m.polarity == Polarity::Umbra) {
        return plan(base_max_rank, innate_slots, mods, inv, stance).or_else(|_| {
            plan(
                base_max_rank,
                innate_slots,
                mods,
                Investment { use_umbra: true, ..inv },
                stance,
            )
        });
    }
    plan(base_max_rank, innate_slots, mods, inv, stance)
}

/// One attempt, with the stance slot's own question answered inside it.
fn plan(
    base_max_rank: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
    inv: Investment,
    stance: Option<StanceSlot>,
) -> Result<Fitted, String> {
    match stance {
        Some(s) => best_stance_plan(base_max_rank, innate_slots, mods, inv, s),
        None => fit_exactly(base_max_rank, innate_slots, mods, inv, None),
    }
}

/// THE TWO ANSWERS A STANCE SLOT HAS, ranked by the same priority the rest of
/// the planner follows: fewest Forma first, then the most room left over.
///
/// Polarizing the slot is worth a flat +5 capacity, and it costs one
/// polarization — which on a rank-40 weapon is one the build was buying anyway.
/// Halving the biggest unpolarized mod is what that Forma would otherwise buy,
/// so the two are compared rather than one being assumed better.
fn best_stance_plan(
    base_max_rank: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
    inv: Investment,
    stance: StanceSlot,
) -> Result<Fitted, String> {
    let as_is = fit_exactly(base_max_rank, innate_slots, mods, inv, Some(stance));
    if stance.matched() {
        return as_is;
    }
    let polarized = fit_exactly(
        base_max_rank,
        innate_slots,
        mods,
        inv,
        Some(StanceSlot { slot_polarity: Some(stance.mod_polarity), ..stance }),
    )
    .map(|mut f| {
        // THE FORMA IT COSTS, on the bill and against the mastery floor — a
        // polarization the build owns is a polarization it paid for.
        f.cost.regular += 1;
        f
    });
    match (as_is, polarized) {
        (Ok(a), Ok(b)) => Ok(better(a, b)),
        (Ok(a), Err(_)) => Ok(a),
        (Err(_), Ok(b)) => Ok(b),
        (Err(e), Err(_)) => Err(e),
    }
}

/// Fewest Forma, then the most capacity left unspent.
fn better(a: Fitted, b: Fitted) -> Fitted {
    let key = |f: &Fitted| (f.cost.total(), f.drain as i64 - f.capacity as i64);
    if key(&b) < key(&a) { b } else { a }
}

/// [`fit`] with the switches taken literally — no Umbra fallback.
fn fit_exactly(
    base_max_rank: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
    inv: Investment,
    stance: Option<StanceSlot>,
) -> Result<Fitted, String> {
    let granted =
        stance.map_or(0, |s| stance_capacity(s.mod_polarity, s.slot_polarity));
    let max_forma = forma_to_max_rank(base_max_rank);
    let plan_at = |forma: u32| -> Result<(u32, u32, FormaPlan), String> {
        let rank = rank_after(base_max_rank, forma);
        let cap = capacity(rank, inv.catalyst) + granted;
        plan_forma_with(cap, innate_slots, mods, inv).map(|p| (rank, cap, p))
    };

    // A rank-30 weapon gains nothing from Forma but a polarity, so its capacity
    // is fixed and one pass answers it.
    if max_forma == 0 || inv.polarize_to_max {
        let spend = if inv.polarize_to_max { max_forma } else { 0 };
        let rank = rank_after(base_max_rank, spend);
        let capacity = capacity(rank, inv.catalyst) + granted;
        // `spend` is a FLOOR, not a target: the planner takes at least that
        // many and more if the build needs them, and it puts every one of them
        // on the biggest mod still unpolarized.
        let plan = plan_forma_spending(capacity, innate_slots, mods, inv, spend)?;
        let mut cost = plan.cost;
        // A build with fewer mods than mastery has polarizations still BUYS
        // them all — the last ones land on empty slots and buy no capacity, but
        // they are bought. The bill is what you spend, not what it earned.
        cost.regular += spend.saturating_sub(cost.total());
        return Ok(Fitted { rank, capacity, drain: plan.total_drain, cost, slots: plan.slots });
    }

    // FIVE IS A CAP ON RANK, NOT ON FORMA. You may polarize as many slots as
    // you have; only the first five raise the max rank. So a self-consistent
    // answer is one where the rank claimed comes from polarizations actually
    // spent — `min(spent, 5)` of them — and spending MORE than five is fine.
    //
    // Try the ceiling first, because it is the common case: a build heavy
    // enough to care about rank-40 capacity is a build that will spend at
    // least five.
    if let Ok((rank, capacity, plan)) = plan_at(max_forma) {
        if plan.cost.total() >= max_forma {
            return Ok(Fitted { rank, capacity, drain: plan.total_drain, cost: plan.cost, slots: plan.slots });
        }
    }
    // Otherwise the build stops short of the ceiling, and the answer is the
    // smallest budget that pays for exactly the rank it assumed.
    let mut last_err = None;
    for f in 0..=max_forma {
        match plan_at(f) {
            Ok((rank, capacity, plan)) if plan.cost.total() == f => {
                return Ok(Fitted { rank, capacity, drain: plan.total_drain, cost: plan.cost, slots: plan.slots });
            }
            Ok(_) => {}
            Err(e) => last_err = Some(e),
        }
    }
    let (rank, capacity, plan) = plan_at(max_forma).map_err(|e| last_err.unwrap_or(e))?;
    Ok(Fitted { rank, capacity, drain: plan.total_drain, cost: plan.cost, slots: plan.slots })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(drain: u32, pol: Polarity) -> PlannedMod {
        PlannedMod { base_drain: drain, polarity: pol }
    }
    const SLOTS8: [Option<Polarity>; 8] = [None; 8];

    /// THE WIKI'S OWN NUMBERS, pinned. Each line is a quote:
    /// "correlates to their Rank... normally 30, but for some items it is 40";
    /// a Catalyst "doubles the available Mod capacity"; and "max rank caps at
    /// 40 after 5 polarizations (max rank increases by 2 per Forma added)".
    #[test]
    fn capacity_follows_rank_and_forma_moves_rank_only_on_a_rank_40_weapon() {
        assert_eq!(capacity(30, false), 30);
        assert_eq!(capacity(30, true), 60, "the constant that was hardcoded");
        assert_eq!(capacity(40, true), 80);

        // A rank-30 weapon gains a polarity from Forma and nothing else.
        for f in 0..=8 {
            assert_eq!(rank_after(30, f), 30, "forma {f}");
        }
        // A rank-40 weapon climbs two at a time and stops at its own ceiling.
        assert_eq!(
            (0..=6).map(|f| rank_after(40, f)).collect::<Vec<_>>(),
            vec![30, 32, 34, 36, 38, 40, 40]
        );
        assert_eq!(forma_to_max_rank(30), 0);
        assert_eq!(forma_to_max_rank(40), 5, "five polarizations, per the wiki");
    }

    /// OMNI matches everything except an Umbra mod — which is the whole reason
    /// it is a polarity rather than a flag on the slot.
    #[test]
    fn omni_matches_anything_but_umbra() {
        assert_eq!(slot_drain(10, Polarity::Madurai, Some(Polarity::Omni)), 5);
        assert_eq!(slot_drain(10, Polarity::Zenurik, Some(Polarity::Omni)), 5);
        // An Umbra mod in an Omni slot is an ordinary MISMATCH, +25%.
        assert_eq!(slot_drain(10, Polarity::Umbra, Some(Polarity::Omni)), 13);
        // ...and an Umbra slot still halves its own mod.
        assert_eq!(slot_drain(10, Polarity::Umbra, Some(Polarity::Umbra)), 5);
    }

    /// The Forma bill is THREE numbers because they are three different items:
    /// a player with Forma may still have no Umbra Forma.
    /// THE STANCE SLOT IS A SLOT THE PLANNER POLARIZES, and it wins the Forma
    /// when what that Forma would otherwise halve is small.
    ///
    /// Polarizing the slot is worth a flat FIVE — the grant doubling — where
    /// polarizing a mod is worth half its drain. So an 8-drain mod's Forma buys
    /// 4 and the stance's buys 5, and the planner takes the five: same bill,
    /// more room left, which is priority 3 in the strategy above.
    ///
    /// …AND IT DOES NOT TAKE IT WHEN THE MOD IS BIGGER. A 16-drain mod's Forma
    /// buys 8, so a build of those fits in FEWER polarizations without touching
    /// the stance slot — priority 2, which outranks the room.
    #[test]
    fn the_planner_polarizes_the_stance_slot_when_that_leaves_more_room() {
        let unmatched = StanceSlot {
            mod_polarity: Polarity::Madurai,
            slot_polarity: Some(Polarity::Vazarin),
        };
        // 66 drain against rank 30's 60 + the 4 a mismatched stance grants.
        let small = [
            m(9, Polarity::Naramon), m(9, Polarity::Naramon),
            m(8, Polarity::Naramon), m(8, Polarity::Naramon),
            m(8, Polarity::Naramon), m(8, Polarity::Naramon),
            m(8, Polarity::Naramon), m(8, Polarity::Naramon),
        ];
        let f = fit(30, &SLOTS8, &small, Investment::default(), Some(unmatched)).unwrap();
        assert_eq!(f.cost.total(), 1, "one polarization either way");
        assert_eq!(f.capacity, 70, "and it went on the stance slot: 60 + a doubled 10");

        // …AND THE OTHER WAY. Eight 16s need eight polarizations whatever the
        // stance slot does, so buying a ninth for five points is a Forma spent
        // to be worse off.
        let big = [m(16, Polarity::Naramon); 8];
        let f = fit(30, &SLOTS8, &big, Investment::default(), Some(unmatched)).unwrap();
        assert_eq!(f.capacity, 64, "the slot keeps its own colour: 60 + a mismatched 4");
        assert_eq!(f.cost.total(), 8, "eight mods, eight Forma, and none on the stance");
    }

    /// A STANCE IS AN AURA: IT HANDS CAPACITY BACK, AND THE SLOT DECIDES HOW
    /// MUCH.
    ///
    /// *"All Stances provide a bonus mod capacity of 5 when maxed, doubling it
    /// to 10 when placed on the matching polarity"* (wiki, Stance), and the
    /// Aura page carries the third case: a slot of a DIFFERENT polarity grants
    /// *"80% of listed drain, rounded down"*, which is 4. A wrong colour costs
    /// something rather than merely failing to double.
    #[test]
    fn a_stance_grants_capacity_and_a_matching_slot_doubles_it() {
        assert_eq!(stance_capacity(Polarity::Madurai, Some(Polarity::Madurai)), 10);
        assert_eq!(stance_capacity(Polarity::Madurai, Some(Polarity::Vazarin)), 4);
        assert_eq!(stance_capacity(Polarity::Madurai, None), 5);
        assert_eq!(stance_capacity(Polarity::Madurai, Some(Polarity::Omni)), 10);

        // …AND IT IS CAPACITY THE BUILD CAN SPEND. Three 16-drain mods on rank
        // 30's 60 need one Forma; the ten a matched stance hands back pay for
        // the third mod instead.
        let mods = [
            m(16, Polarity::Madurai),
            m(16, Polarity::Naramon),
            m(16, Polarity::Vazarin),
            m(16, Polarity::Zenurik),
        ];
        let bare = fit(30, &SLOTS8, &mods, Investment::default(), None).unwrap();
        let matched = StanceSlot {
            mod_polarity: Polarity::Madurai,
            slot_polarity: Some(Polarity::Madurai),
        };
        let stanced = fit(30, &SLOTS8, &mods, Investment::default(), Some(matched)).unwrap();
        assert_eq!(stanced.capacity, bare.capacity + 10, "the grant is capacity");
        assert!(
            stanced.cost.total() < bare.cost.total(),
            "and capacity the build can spend: {} Forma against {}",
            stanced.cost.total(),
            bare.cost.total(),
        );
    }

    #[test]
    fn the_forma_bill_is_split_by_the_item_it_costs() {
        let mods = [m(16, Polarity::Madurai), m(16, Polarity::Naramon)];
        let plain = plan_forma_with(20, &SLOTS8, &mods, Investment::default()).unwrap();
        assert_eq!(plain.cost, FormaCost { regular: 2, omni: 0, umbra: 0 });

        // Choosing Omni is all-or-nothing: every polarized slot becomes one.
        let omni = Investment { use_omni: true, ..Investment::default() };
        let p = plan_forma_with(20, &SLOTS8, &mods, omni).unwrap();
        assert_eq!(p.cost, FormaCost { regular: 0, omni: 2, umbra: 0 });
        assert_eq!(p.slots, vec![Some(Polarity::Omni), Some(Polarity::Omni)]);
    }

    /// UMBRA FORMA IS SCARCE, so the default refuses to spend it — and an
    /// unmatched Umbra mod pays full drain rather than blocking the build.
    #[test]
    fn umbra_forma_is_only_spent_when_allowed() {
        let mods = [m(16, Polarity::Umbra), m(10, Polarity::Madurai)];
        // 16 full + 10 halved = 21: fits 22 without ever touching Umbra Forma.
        let off = plan_forma_with(22, &SLOTS8, &mods, Investment::default()).unwrap();
        assert_eq!(off.cost, FormaCost { regular: 1, omni: 0, umbra: 0 });
        assert_eq!(off.slots[0], None, "the Umbra mod stays unpolarized");

        // Allowed, it is matched like anything else — and billed as Umbra.
        let on = Investment { use_umbra: true, ..Investment::default() };
        let p = plan_forma_with(13, &SLOTS8, &mods, on).unwrap();
        assert_eq!(p.cost, FormaCost { regular: 1, omni: 0, umbra: 1 });

        // Refused AND needed: the error says which switch is in the way, since
        // "needs more capacity" alone would send you looking at the mods.
        let e = plan_forma_with(13, &SLOTS8, &mods, Investment::default()).unwrap_err();
        assert!(e.contains("Umbra Forma is switched off"), "{e}");
    }

    /// THE BILL FOR A LAYOUT YOU SET, which is not the same question as "what
    /// would the cheapest be". Innate polarities are a pool a Forma may
    /// REPOSITION, so an add and a remove cancel — but BLANKING one still costs
    /// a Forma, which is why the bill is `max(added, removed)` and not the sum.
    #[test]
    fn the_bill_for_a_layout_repositions_innates_for_free() {
        let innate = [Some(Polarity::Madurai), Some(Polarity::Naramon), None, None];
        let at = |pol: Option<Polarity>, mod_pol: Polarity| Placement {
            base_drain: 10, mod_polarity: mod_pol, slot_polarity: pol,
        };

        // The build uses exactly what the weapon was born with, in the other
        // order: repositioning is free.
        let (drain, cost) = cost_of(
            &innate,
            &[at(Some(Polarity::Naramon), Polarity::Naramon),
              at(Some(Polarity::Madurai), Polarity::Madurai)],
        );
        assert_eq!(drain, 10);
        assert_eq!(cost, FormaCost::default(), "a swap costs nothing");

        // One innate BLANKED and nothing added: still one Forma, because
        // removing a polarity is itself a polarization.
        let (_, cost) = cost_of(&innate, &[at(Some(Polarity::Madurai), Polarity::Madurai)]);
        assert_eq!(cost.regular, 1, "blanking the Naramon costs one");

        // A colour swap is ONE Forma, not two: the add and the remove are the
        // same act.
        let (_, cost) = cost_of(
            &innate,
            &[at(Some(Polarity::Vazarin), Polarity::Vazarin),
              at(Some(Polarity::Naramon), Polarity::Naramon)],
        );
        assert_eq!(cost.regular, 1);

        // Umbra and Omni are billed apart.
        let (_, cost) = cost_of(
            &innate,
            &[at(Some(Polarity::Umbra), Polarity::Umbra),
              at(Some(Polarity::Omni), Polarity::Madurai),
              at(Some(Polarity::Madurai), Polarity::Madurai),
              at(Some(Polarity::Naramon), Polarity::Naramon)],
        );
        assert_eq!(cost, FormaCost { regular: 0, omni: 1, umbra: 1 });
    }

    /// AN INNATE UMBRA IS NOT A PURCHASE. Some weapons are born with one, and
    /// keeping it costs nothing — the bill charged for it until 2026-08-04.
    #[test]
    fn an_innate_umbra_slot_is_kept_not_bought() {
        let innate = [Some(Polarity::Umbra), Some(Polarity::Madurai), None, None];
        let at = |pol: Option<Polarity>, mod_pol: Polarity| Placement {
            base_drain: 10, mod_polarity: mod_pol, slot_polarity: pol,
        };
        // EVERY slot, including the ones with no mod — an innate polarity on an
        // empty slot is still there, and reading the build as "the rest were
        // blanked" charges a Forma for nothing.
        let empty = |pol: Option<Polarity>| Placement {
            base_drain: 0, mod_polarity: Polarity::Madurai, slot_polarity: pol,
        };
        // Using the slot the weapon came with: nothing bought.
        let (_, cost) = cost_of(
            &innate,
            &[at(Some(Polarity::Umbra), Polarity::Umbra), empty(Some(Polarity::Madurai))],
        );
        assert_eq!(cost, FormaCost::default(), "the weapon came with it");

        // A SECOND Umbra slot is one purchase, not two.
        let (_, cost) = cost_of(
            &innate,
            &[at(Some(Polarity::Umbra), Polarity::Umbra),
              at(Some(Polarity::Umbra), Polarity::Umbra),
              empty(Some(Polarity::Madurai))],
        );
        assert_eq!(cost.umbra, 1);
    }

    /// UMBRA IS SPENT ONLY WHEN REFUSING WOULD BE WRONG. Not a veto — a first answer.
    #[test]
    fn umbra_forma_is_a_last_resort_rather_than_a_refusal() {
        let innate = [None; 8];
        // Room to spare: the Umbra mod pays full drain and no Umbra Forma is
        // bought, which is the thrifty answer and the right one.
        let easy = [m(16, Polarity::Umbra), m(10, Polarity::Madurai)];
        let f = fit(30, &innate, &easy, Investment::default(), None).unwrap();
        assert_eq!(f.cost.umbra, 0, "it fits without one, so none is spent");

        // Now a build that hangs on exactly this: one Umbra mod at 16 and six
        // others at 16, on rank 30's cap of 60.
        //   without Umbra Forma:  16 + 6x8 = 64  -> impossible
        //   with it:               8 + 6x8 = 56  -> fits
        // Refusing would invent a rule the game does not have.
        let mut hard = vec![m(16, Polarity::Umbra)];
        hard.extend(std::iter::repeat_n(m(16, Polarity::Madurai), 6));
        let f = fit(30, &innate, &hard, Investment::default(), None)
            .expect("a build that NEEDS Umbra Forma is still a build");
        assert_eq!(f.cost.umbra, 1, "exactly one, and only because it had to");
        assert_eq!(f.drain, 56);
        assert!(f.drain <= f.capacity);
    }

    /// A rank-40 weapon polarized to max has its capacity BEFORE planning
    /// starts — which is what makes the default path free of any solving.
    #[test]
    fn polarizing_to_max_fixes_the_capacity_first() {
        let mods = [m(16, Polarity::Madurai), m(16, Polarity::Naramon), m(16, Polarity::Vazarin)];
        let f = fit(40, &SLOTS8, &mods, Investment::default(), None).unwrap();
        assert_eq!((f.rank, f.capacity), (40, 80));
        // The five are spent for mastery either way, so they are PUT TO WORK:
        // all three mods halved (48 -> 24) rather than left at full drain
        // because the build "did not need it". Same Forma, 56 spare instead of
        // 32 — which is the point of spending them.
        assert_eq!(f.drain, 24, "every polarization that is bought is used");
        assert_eq!(f.cost.total(), 5, "five polarizations for full affinity");
        assert_eq!(f.cost, FormaCost { regular: 5, omni: 0, umbra: 0 });
        assert_eq!(f.capacity - f.drain, 56, "and the room they buy is the most they could");
    }

    /// WITHOUT that default the loop is real, and the answer is the smallest
    /// budget that covers its own consequences.
    #[test]
    fn without_polarizing_to_max_the_budget_has_to_cover_itself() {
        let thrifty = Investment { polarize_to_max: false, ..Investment::default() };
        // Comfortably inside rank 30's 60: no Forma, no rank gained.
        let easy = [m(16, Polarity::Madurai), m(16, Polarity::Naramon), m(16, Polarity::Vazarin)];
        let f = fit(40, &SLOTS8, &easy, thrifty, None).unwrap();
        assert_eq!((f.rank, f.capacity, f.cost.total()), (30, 60, 0));

        // Eight 16-drain mods: 128 full, 64 halved. At rank 30 (60) even eight
        // Forma cannot fit it; the capacity the Forma THEMSELVES buy is what
        // makes it possible, which is the loop this test exists for.
        // Eight 16-drain mods: 128 full, 64 halved. Rank 30's 60 cannot hold
        // it however many slots are polarized — the capacity the Forma
        // THEMSELVES buy is what makes it possible, which is the loop this
        // test exists for.
        let heavy = [m(16, Polarity::Madurai); 8];
        let f = fit(40, &SLOTS8, &heavy, thrifty, None).unwrap();
        assert_eq!((f.rank, f.capacity), (40, 80), "it had to buy rank to fit");
        assert!(f.drain <= f.capacity, "drain {} cap {}", f.drain, f.capacity);
        // SIX polarizations, and six is not a contradiction: five is the cap on
        // RANK, not on how many slots may be Forma'd. The sixth buys no rank
        // and is spent anyway, which is exactly what the game does.
        assert_eq!(f.cost.total(), 6, "{:?}", f.cost);
    }

    #[test]
    fn matched_polarity_halves_rounding_up() {
        assert_eq!(
            slot_drain(11, Polarity::Madurai, Some(Polarity::Madurai)),
            6
        );
        assert_eq!(
            slot_drain(10, Polarity::Naramon, Some(Polarity::Naramon)),
            5
        );
        assert_eq!(slot_drain(7, Polarity::Vazarin, Some(Polarity::Vazarin)), 4);
    }

    #[test]
    fn mismatched_polarity_adds_a_quarter() {
        assert_eq!(
            slot_drain(11, Polarity::Madurai, Some(Polarity::Naramon)),
            14
        ); // 13.75
        assert_eq!(
            slot_drain(16, Polarity::Madurai, Some(Polarity::Vazarin)),
            20
        );
        assert_eq!(slot_drain(9, Polarity::Umbra, Some(Polarity::Madurai)), 11);
        // 11.25
        // MEASURED: the half case rounds UP.
        assert_eq!(
            slot_drain(10, Polarity::Madurai, Some(Polarity::Naramon)),
            13
        ); // 12.5 -> 13
    }

    #[test]
    fn unpolarized_slot_charges_full_drain() {
        assert_eq!(slot_drain(11, Polarity::Madurai, None), 11);
    }

    #[test]
    fn capacity_doubles_with_a_catalyst() {
        assert_eq!(capacity(30, false), 30);
        assert_eq!(capacity(30, true), 60);
    }

    #[test]
    fn loadout_validation_enforces_the_cap() {
        // Dual Toxocyst fully unlocked: rank 30 + catalyst = 60 capacity;
        // innate Madurai + Naramon slots, Naramon exilus.
        let cap = capacity(30, true);
        let ok = [
            // e.g. a rank-10 Madurai mod (drain 14) in the Madurai slot -> 7
            Placement {
                base_drain: 14,
                mod_polarity: Polarity::Madurai,
                slot_polarity: Some(Polarity::Madurai),
            },
            // rank-10 Naramon mod in the Naramon slot -> 6
            Placement {
                base_drain: 11,
                mod_polarity: Polarity::Naramon,
                slot_polarity: Some(Polarity::Naramon),
            },
            // unpolarized slots at full drain
            Placement {
                base_drain: 11,
                mod_polarity: Polarity::Madurai,
                slot_polarity: None,
            },
            Placement {
                base_drain: 9,
                mod_polarity: Polarity::Vazarin,
                slot_polarity: None,
            },
        ];
        assert_eq!(validate_loadout(cap, &ok), Ok(7 + 6 + 11 + 9));

        // Cramming mismatches until it bursts is an error, not a warning:
        // 16-drain mods in wrong-polarity slots cost 20 each.
        let mismatch = Placement {
            base_drain: 16,
            mod_polarity: Polarity::Madurai,
            slot_polarity: Some(Polarity::Vazarin),
        };
        assert_eq!(validate_loadout(60, &[mismatch; 3]), Ok(60)); // exactly full
        assert!(validate_loadout(60, &[mismatch; 4]).is_err()); // 80 > 60
    }

    #[test]
    fn auto_forma_fits_the_proposed_dt_build_with_four_forma() {
        // Dual Toxocyst: innate pool [Madurai, Naramon], 8 slots, cap 60.
        // Proposed 8: Hornet 14M, PTC 14M, GalvDiffusion 14M, PPG 12M,
        // GalvShot 12V, Lethal Torrent 11M, Frostbite 7M, Jolt 7M.
        let m = |d, p| PlannedMod {
            base_drain: d,
            polarity: p,
        };
        use Polarity::*;
        let mods = [
            m(14, Madurai),
            m(14, Madurai),
            m(14, Madurai),
            m(12, Madurai),
            m(12, Vazarin),
            m(11, Madurai),
            m(7, Madurai),
            m(7, Madurai),
        ];
        let innate = [
            Some(Madurai),
            Some(Naramon),
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        let plan = plan_forma(60, &innate, &mods).unwrap();
        // Innate Madurai halves one 14 (7); the Naramon polarity finds no
        // taker. Forma greedily: 14->7, 14->7, 12->6, 12->6 = 4 Forma.
        // Total: 7+7+7+6+6+11+7+7 = 58 <= 60.
        assert_eq!(plan.forma_used, 4, "plan: {plan:?}");
        assert_eq!(plan.total_drain, 58);
    }

    #[test]
    fn auto_forma_rejects_impossible_builds() {
        let m = PlannedMod {
            base_drain: 16,
            polarity: Polarity::Madurai,
        };
        // Eight 16-drain mods fully forma'd still need 8 x 8 = 64 > 60.
        let innate = [None; 8];
        assert!(plan_forma(60, &innate, &[m; 8]).is_err());
    }
}
