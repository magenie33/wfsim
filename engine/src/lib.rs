// SPDX-License-Identifier: AGPL-3.0-or-later
//! The WFSim engine: every game mechanic, and no weapon names — those load
//! from `data/`.
//!
//! Six layers, each naming only the ones before it (docs/CORE.md §4):
//! `rules` — primitives and formulas · `model` — the vocabulary · `data` — the
//! catalogs · `build` — a loadout resolved into numbers · the fight — `target`,
//! `formation`, `arena`, `fight`, `record` · `board` — build identity and the
//! rulers. `scripts/check_engine_layers.mjs` fails a module that reaches up.

pub mod naming;

pub mod rules;

pub mod model;

pub mod data;

pub mod build;

pub mod target;
pub mod formation;
pub mod arena;
pub mod record;
pub mod fight;

pub mod board;
