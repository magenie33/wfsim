// SPDX-License-Identifier: AGPL-3.0-or-later
//! A browser leader parks measurements, but only promoted candidates need a decoded shard.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wfsim_engine::fight::Shard;

#[derive(Clone, Serialize)]
#[serde(untagged)]
pub(super) enum Carry {
    Dense(Box<Shard>),
    Packed(String),
}

impl Carry {
    pub(super) fn new(shard: Shard, pack: bool) -> Self {
        if pack { Self::Packed(shard.pack_search()) } else { Self::Dense(Box::new(shard)) }
    }

    pub(super) fn read(v: &Value) -> Option<Self> {
        if let Some(s) = v.as_str() { return Some(Self::Packed(s.to_owned())); }
        Box::<Shard>::deserialize(v).ok().map(Self::Dense)
    }

    pub(super) fn list(v: &Value) -> Option<Vec<Option<Self>>> {
        v.as_array()?.iter().map(|v| if v.is_null() { Some(None) } else { Self::read(v).map(Some) }).collect()
    }

    pub(super) fn open(self) -> Option<Shard> {
        match self { Self::Dense(s) => Some(*s), Self::Packed(s) => Shard::unpack_search(&s) }
    }
}
