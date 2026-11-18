// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE ENGINE'S TRANSCENDENTAL MATHS, one implementation on every target.
//!
//! `f64::powf`, `exp`, `sin`… call the platform's maths library — the MSVC CRT,
//! glibc, or compiler-builtins in wasm — and IEEE does not fix their last bit,
//! so two targets may give two answers. Every such call in the engine goes
//! through [`Portable`], backed by the pure-Rust `libm` crate, so a native build
//! and the wasm compute the same bits by construction (docs/DESKTOP.md).
//! `sqrt`, `abs`, `floor`, `mul_add` and the like are exact under IEEE and stay
//! as they are. `check_gas` refuses a direct call anywhere else.
//!
//! Methods rather than functions, so a call site weighs what the std call did
//! and the gas meter's counts do not move (docs/WASM.md §"Gas").

/// Native stat curves evaluate their power operation in binary32 (M8).
pub fn powf_f32(base: f32, exponent: f32) -> f32 {
    libm::powf(base, exponent)
}

/// An `f64` method per transcendental the engine uses, each named for the std
/// method it replaces.
pub trait Portable {
    fn powf_portable(self, e: f64) -> f64;
    fn powi_portable(self, n: i32) -> f64;
    fn exp_portable(self) -> f64;
    fn ln_portable(self) -> f64;
    fn log10_portable(self) -> f64;
    fn sin_portable(self) -> f64;
    fn cos_portable(self) -> f64;
    fn sin_cos_portable(self) -> (f64, f64);
    fn acos_portable(self) -> f64;
    fn atan2_portable(self, x: f64) -> f64;
    fn hypot_portable(self, y: f64) -> f64;
}

pub(super) trait LevelPower {
    fn level_power(self, exponent: f64) -> f64;
}

impl LevelPower for f64 {
    /// Level curves repeatedly ask for the same powers on every hit and status tick.
    /// Both operands are keyed by bits; a collision only evicts a previous result.
    #[inline]
    fn level_power(self, exponent: f64) -> f64 {
        use std::cell::Cell;
        type Entry = (u64, u64, f64);
        thread_local! {
            static POWERS: [Cell<Entry>; 32] = const { [const { Cell::new((0, 0, 1.0)) }; 32] };
        }
        let (x, e) = (self.to_bits(), exponent.to_bits());
        let hash = e ^ (e >> 32) ^ x.rotate_left(17);
        let index = (hash.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 59) as usize;
        POWERS.with(|table| {
            let (a, b, value) = table[index].get();
            if (a, b) == (x, e) { return value; }
            let value = libm::pow(self, exponent);
            table[index].set((x, e, value));
            value
        })
    }
}

impl Portable for f64 {
    fn powf_portable(self, e: f64) -> f64 {
        libm::pow(self, e)
    }
    /// compiler-rt's `__powidf2`, which `f64::powi` lowers to on every target
    /// and which LLVM's constant-exponent expansion multiplies in the same order.
    fn powi_portable(self, n: i32) -> f64 {
        let (mut a, mut b, mut r) = (self, n, 1.0);
        loop {
            if b & 1 != 0 {
                r *= a;
            }
            b /= 2;
            if b == 0 {
                break;
            }
            a *= a;
        }
        if n < 0 { 1.0 / r } else { r }
    }
    fn exp_portable(self) -> f64 {
        libm::exp(self)
    }
    fn ln_portable(self) -> f64 {
        libm::log(self)
    }
    fn log10_portable(self) -> f64 {
        libm::log10(self)
    }
    fn sin_portable(self) -> f64 {
        libm::sin(self)
    }
    fn cos_portable(self) -> f64 {
        libm::cos(self)
    }
    /// Two calls, as `f64::sin_cos` makes them.
    fn sin_cos_portable(self) -> (f64, f64) {
        (libm::sin(self), libm::cos(self))
    }
    fn acos_portable(self) -> f64 {
        libm::acos(self)
    }
    fn atan2_portable(self, x: f64) -> f64 {
        libm::atan2(self, x)
    }
    fn hypot_portable(self, y: f64) -> f64 {
        libm::hypot(self, y)
    }
}

#[cfg(test)]
mod tests {
    use super::{LevelPower, Portable};

    #[test]
    fn repeated_level_powers_are_the_same_bits_even_after_eviction() {
        for _ in 0..2 {
            for level in (0..=140).map(f64::from).chain([9998.0, -0.0, f64::INFINITY]) {
                for exponent in [-1.0, 0.0, 0.5, 0.72, 0.75, 1.75, 2.0, 2.12] {
                    for _ in 0..2 {
                        assert_eq!(level.level_power(exponent).to_bits(), libm::pow(level, exponent).to_bits());
                    }
                }
            }
        }
    }
    #[test]
    fn powi_is_repeated_squaring_and_agrees_with_std_where_std_is_exact() {
        for n in -6..=6 {
            assert_eq!(2.0f64.powi_portable(n), 2.0f64.powi(n), "2^{n}");
        }
        assert_eq!(0.75f64.powi_portable(3), 0.75 * (0.75 * 0.75));
        assert_eq!(1.3f64.powi_portable(0), 1.0);
    }

    #[test]
    fn each_method_is_its_std_namesake_to_within_an_ulp() {
        let near = |a: f64, b: f64| (a - b).abs() <= 2.0 * f64::EPSILON * a.abs().max(b.abs()).max(1.0);
        for &x in &[0.1, 0.5, 1.0, 2.5, 80.0, 1234.5] {
            assert!(near(x.powf_portable(1.55), x.powf(1.55)));
            assert!(near(x.ln_portable(), x.ln()) && near((x / 100.0).exp_portable(), (x / 100.0).exp()));
            assert!(near(x.log10_portable(), x.log10()));
            assert!(near(x.sin_portable(), x.sin()) && near(x.cos_portable(), x.cos()));
            assert_eq!(x.sin_cos_portable(), (x.sin_portable(), x.cos_portable()));
            assert!(near(x.atan2_portable(3.0), x.atan2(3.0)) && near(x.hypot_portable(3.0), x.hypot(3.0)));
            assert!(near((x / 2000.0).acos_portable(), (x / 2000.0).acos()));
        }
    }
}
