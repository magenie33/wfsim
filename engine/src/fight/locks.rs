/// A buff forced active by a "buff lock" simulation setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockedBuff {
    Frenzy,
}

/// Per-buff lock setting: each buff is configured
/// INDEPENDENTLY —
/// - `Permanent`: re-asserted every shot, overriding natural expiry
///   (100% uptime, full stacks).
/// - `Initial(stacks)`: granted once at t = 0 at the given stack count
///   with its NATURAL duration; afterwards only the buff's own mechanics
///   (triggers, decay, expiry) govern it. For non-stacking buffs
///   (Frenzy) the count is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Permanent,
    Initial(u32),
}

/// One buff-lock setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffLock {
    pub buff: LockedBuff,
    pub mode: LockMode,
}

#[wfsim_gas_macro::metered]
impl BuffLock {
    pub fn permanent(buff: LockedBuff) -> Self {
        Self {
            buff,
            mode: LockMode::Permanent,
        }
    }

    pub fn initial(buff: LockedBuff, stacks: u32) -> Self {
        Self {
            buff,
            mode: LockMode::Initial(stacks),
        }
    }
}

/// WHAT A FIGHT PUTS ON ITS ENTRANT beside the panels it fires: the reserve,
/// the ammo economy and Frenzy. Built once from the parsed fight and read by
/// [`super::FightParams::for_entrant`] for the simulator and the optimizer
/// alike, so neither can carry a value the other does not.
#[derive(Debug, Clone, PartialEq)]
pub struct EntrantTerms {
    pub infinite_ammo: bool,
    pub ammo_drops: bool,
    pub pickup_range_m: f64,
    pub landscape: bool,
    /// The weapon OWNS the passive and the fight leaves it on.
    pub frenzy: bool,
    pub cycle_frenzy_lock: LockMode,
    pub frenzy_locks: Vec<BuffLock>,
}
