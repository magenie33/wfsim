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
