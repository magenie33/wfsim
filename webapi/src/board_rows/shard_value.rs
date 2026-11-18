// SPDX-License-Identifier: AGPL-3.0-or-later
//! Check the generated shard without constructing a serde_json::Value tree.

use serde::ser::{self, Impossible};
use serde::{Serialize, Serializer};

pub(super) fn lossless(value: &impl Serialize) -> bool {
    value.serialize(Check).is_ok()
}

#[derive(Debug)]
struct NeedsValue;

impl std::fmt::Display for NeedsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("use the legacy shard Value conversion")
    }
}

impl std::error::Error for NeedsValue {}

impl ser::Error for NeedsValue {
    fn custom<T: std::fmt::Display>(_: T) -> Self { Self }
}

#[derive(Clone, Copy)]
struct Check;

// Only the derived Shard shape takes the fast path. If its schema picks up
// a different serializer type, leave that piece to the legacy conversion.
macro_rules! unsupported {
    ($($name:ident($($ty:ty),*);)*) => { $(
        fn $name(self, $(_: $ty),*) -> Result<(), NeedsValue> { Err(NeedsValue) }
    )* };
}

impl Serializer for Check {
    type Ok = ();
    type Error = NeedsValue;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeStruct = Self;
    type SerializeTupleStruct = Impossible<(), NeedsValue>;
    type SerializeTupleVariant = Impossible<(), NeedsValue>;
    type SerializeMap = Impossible<(), NeedsValue>;
    type SerializeStructVariant = Impossible<(), NeedsValue>;

    fn serialize_u32(self, _: u32) -> Result<(), NeedsValue> { Ok(()) }
    // This check replaces a Rust Value round trip, not a JavaScript number
    // round trip. serde_json::Value keeps the full u64.
    fn serialize_u64(self, _: u64) -> Result<(), NeedsValue> { Ok(()) }
    fn serialize_f64(self, n: f64) -> Result<(), NeedsValue> {
        if n.is_finite() && n.to_bits() != (-0.0_f64).to_bits() { Ok(()) } else { Err(NeedsValue) }
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), NeedsValue> {
        value.serialize(self)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self, NeedsValue> { Ok(self) }
    fn serialize_tuple(self, _: usize) -> Result<Self, NeedsValue> { Ok(self) }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, NeedsValue> { Ok(self) }

    unsupported! {
        serialize_bool(bool); serialize_char(char); serialize_str(&str); serialize_bytes(&[u8]);
        serialize_i8(i8); serialize_i16(i16); serialize_i32(i32); serialize_i64(i64); serialize_i128(i128);
        serialize_u8(u8); serialize_u16(u16); serialize_u128(u128); serialize_f32(f32);
        serialize_none(); serialize_unit(); serialize_unit_struct(&'static str);
        serialize_unit_variant(&'static str, u32, &'static str);
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<(), NeedsValue> { Err(NeedsValue) }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> Result<(), NeedsValue> { Err(NeedsValue) }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, NeedsValue> { Err(NeedsValue) }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeTupleVariant, NeedsValue> { Err(NeedsValue) }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, NeedsValue> { Err(NeedsValue) }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeStructVariant, NeedsValue> { Err(NeedsValue) }
}

macro_rules! compound {
    ($kind:ident, $method:ident $(, $key:ty)?) => {
        impl ser::$kind for Check {
            type Ok = ();
            type Error = NeedsValue;

            fn $method<T: Serialize + ?Sized>(&mut self, $(_: $key,)? value: &T) -> Result<(), NeedsValue> {
                value.serialize(*self)
            }

            fn end(self) -> Result<(), NeedsValue> { Ok(()) }
        }
    };
}

compound!(SerializeSeq, serialize_element);
compound!(SerializeTuple, serialize_element);
compound!(SerializeStruct, serialize_field, &'static str);

#[cfg(test)]
#[test]
fn exceptional_numbers_keep_using_value() {
    assert!(lossless(&u64::MAX));
    assert!(lossless(&[0.0, f64::MAX, f64::from_bits(1)]));
    assert!(!lossless(&[0.0, -0.0]));
    assert!(!lossless(&[f64::INFINITY, 1.0]));
    assert!(!lossless(&[1.0, f64::NAN]));
}
