// SPDX-License-Identifier: AGPL-3.0-or-later
//! `#[metered]` — THE GAS METER, AT THE SOURCE. On a function, an `impl`, a
//! `trait` or an inline `mod`, every function body charges its WEIGHT once per
//! call and every loop its body's weight each turn, to the engine's count
//! (`crate::fight::gas`). A weight is counted from the source — expressions
//! and statements, a closure's body once where it is written, a loop's body
//! left to the loop — so it is the same on every target, at every
//! optimisation level, inlined or not. docs/WASM.md §"Gas".

use proc_macro::TokenStream;
use quote::quote;
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{parse_macro_input, parse_quote, Block, Expr, Item, Stmt};

/// THE WEIGHT OF A REGION: its expressions and statements, a closure's body
/// among them — charged once where it is written, however often it is
/// called — and not what runs a variable number of times (a loop's body, a
/// `while` condition) or belongs elsewhere (a nested item).
#[derive(Default)]
struct Weigh(u64);

impl<'a> Visit<'a> for Weigh {
    fn visit_expr(&mut self, e: &'a Expr) {
        self.0 += 1;
        match e {
            Expr::ForLoop(f) => self.visit_expr(&f.expr),
            Expr::While(_) | Expr::Loop(_) => {}
            _ => syn::visit::visit_expr(self, e),
        }
    }
    fn visit_stmt(&mut self, s: &'a Stmt) {
        self.0 += 1;
        syn::visit::visit_stmt(self, s);
    }
    fn visit_item(&mut self, _: &'a Item) {}
}

fn weigh_block(b: &Block) -> u64 {
    let mut w = Weigh::default();
    for s in &b.stmts {
        w.visit_stmt(s);
    }
    w.0
}

/// A BODY THIS LIGHT, with no loop and no closure, charges nothing: an
/// accessor's cost is its caller's call, and charging it would cost more than it.
const SMALL: u64 = 8;

fn charge(n: u64) -> Stmt {
    parse_quote!(crate::fight::gas::charge(#n);)
}

/// A loop's charge in a function body: to the body's own tally, a local the
/// optimiser keeps in a register, paid into the count once as it returns.
fn tally(n: u64) -> Stmt {
    parse_quote!(__wfsim_gas.add(#n);)
}

/// Prepends each body's charge and each loop's. A loop charges the function's
/// tally, which then never leaves the function's frame; a loop inside a
/// closure, which would have to borrow it, charges the count.
#[derive(Default)]
struct Meter {
    in_closure: u32,
    tallied: bool,
}

impl Meter {
    fn loop_charge(&mut self, n: u64) -> Stmt {
        if self.in_closure == 0 {
            self.tallied = true;
            tally(n)
        } else {
            charge(n)
        }
    }
    /// A function body: its loops tally, and the tally opens with its weight.
    fn body(&mut self, b: &mut Block) {
        let outer = std::mem::take(&mut self.tallied);
        let saved = std::mem::take(&mut self.in_closure);
        self.visit_block_mut(b);
        let n = weigh_block(b) + 1;
        if self.tallied {
            b.stmts.insert(0, parse_quote!(let __wfsim_gas = crate::fight::gas::Tally::new(#n);));
        } else if n > SMALL {
            b.stmts.insert(0, charge(n));
        }
        self.tallied = outer;
        self.in_closure = saved;
    }
}

impl VisitMut for Meter {
    fn visit_expr_mut(&mut self, e: &mut Expr) {
        let closure = matches!(e, Expr::Closure(_));
        self.in_closure += u32::from(closure);
        syn::visit_mut::visit_expr_mut(self, e);
        self.in_closure -= u32::from(closure);
        match e {
            Expr::ForLoop(f) => {
                let n = weigh_block(&f.body) + 1;
                let c = self.loop_charge(n);
                f.body.stmts.insert(0, c);
            }
            Expr::Loop(l) => {
                let n = weigh_block(&l.body) + 1;
                let c = self.loop_charge(n);
                l.body.stmts.insert(0, c);
            }
            Expr::While(w) => {
                // The condition runs once more than the body; the body pays for both.
                let mut c = Weigh::default();
                c.visit_expr(&w.cond);
                let n = weigh_block(&w.body) + c.0 + 1;
                let c = self.loop_charge(n);
                w.body.stmts.insert(0, c);
            }
            _ => {}
        }
    }
    fn visit_item_fn_mut(&mut self, f: &mut syn::ItemFn) {
        if f.sig.constness.is_none() {
            self.body(&mut f.block);
        }
    }
    fn visit_impl_item_fn_mut(&mut self, f: &mut syn::ImplItemFn) {
        if f.sig.constness.is_none() {
            self.body(&mut f.block);
        }
    }
    fn visit_trait_item_fn_mut(&mut self, f: &mut syn::TraitItemFn) {
        if f.sig.constness.is_none() {
            if let Some(b) = &mut f.default {
                self.body(b);
            }
        }
    }
}

#[proc_macro_attribute]
pub fn metered(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item = parse_macro_input!(input as Item);
    Meter::default().visit_item_mut(&mut item);
    quote!(#item).into()
}
