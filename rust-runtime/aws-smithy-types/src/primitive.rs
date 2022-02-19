/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Utilities for formatting and parsing primitives
//!
//! Smithy protocols have specific behavior for serializing
//! & deserializing floats, specifically:
//! - NaN should be serialized as `NaN`
//! - Positive infinity should be serialized as `Infinity`
//! - Negative infinity should be serialized as `-Infinity`
//!
//! This module defines the [`Parse`](Parse) trait which
//! enables parsing primitive values (numbers & booleans) that follow
//! these rules and [`Encoder`](Encoder), a struct that enables
//! allocation-free serialization.
//!
//! # Examples
//! ## Parsing
//! ```rust
//! use aws_smithy_types::primitive::Parse;
//! let parsed = f64::parse_smithy_primitive("123.4").expect("valid float");
//! ```
//!
//! ## Encoding
//! ```
//! use aws_smithy_types::primitive::Encoder;
//! assert_eq!("123.4", Encoder::from(123.4).encode());
//! assert_eq!("Infinity", Encoder::from(f64::INFINITY).encode());
//! assert_eq!("true", Encoder::from(true).encode());
//! ```
use crate::primitive::private::Sealed;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;

/// An error during primitive parsing
#[non_exhaustive]
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PrimitiveParseError(&'static str);
impl Display for PrimitiveParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to parse input as {}", self.0)
    }
}
impl Error for PrimitiveParseError {}

/// Sealed trait for custom parsing of primitive types
pub trait Parse: Sealed {
    /// Parses a Smithy primitive from a string.
    fn parse_smithy_primitive(input: &str) -> Result<Self, PrimitiveParseError>
    where
        Self: Sized;
}

mod private {
    pub trait Sealed {}
    impl Sealed for i8 {}
    impl Sealed for i16 {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
    impl Sealed for u64 {}
    impl Sealed for bool {}
}

macro_rules! parse_from_str {
    ($t: ty) => {
        impl Parse for $t {
            fn parse_smithy_primitive(input: &str) -> Result<Self, PrimitiveParseError> {
                FromStr::from_str(input).map_err(|_| PrimitiveParseError(stringify!($t)))
            }
        }
    };
}

parse_from_str!(bool);
parse_from_str!(i8);
parse_from_str!(i16);
parse_from_str!(i32);
parse_from_str!(i64);

impl Parse for f32 {
    fn parse_smithy_primitive(input: &str) -> Result<Self, PrimitiveParseError> {
        float::parse_f32(input).map_err(|_| PrimitiveParseError("f32"))
    }
}

impl Parse for f64 {
    fn parse_smithy_primitive(input: &str) -> Result<Self, PrimitiveParseError> {
        float::parse_f64(input).map_err(|_| PrimitiveParseError("f64"))
    }
}

/// This type exists to hide `itoa::Buffer` implementation detail from the public API
#[allow(missing_debug_implementations)]
#[doc(hidden)]
#[derive(Default)]
pub struct IntegerEncoder(itoa::Buffer);

/// This type exists to hide `ryu::Buffer` implementation detail from the public API
#[allow(missing_debug_implementations)]
#[doc(hidden)]
#[derive(Default)]
pub struct FloatingEncoder(ryu::Buffer);

/// Primitive Type Encoder
///
/// Encodes primitive types in Smithy's specified format. For floating-point numbers,
/// Smithy requires that NaN and Infinity values be specially encoded.
///
/// This type implements `From<T>` for all Smithy primitive types.
#[non_exhaustive]
pub enum Encoder {
    /// Boolean
    #[non_exhaustive]
    Bool(bool),
    /// 8-bit signed integer
    #[non_exhaustive]
    I8(i8, IntegerEncoder),
    /// 16-bit signed integer
    #[non_exhaustive]
    I16(i16, IntegerEncoder),
    /// 32-bit signed integer
    #[non_exhaustive]
    I32(i32, IntegerEncoder),
    /// 64-bit signed integer
    #[non_exhaustive]
    I64(i64, IntegerEncoder),
    /// 64-bit unsigned integer
    #[non_exhaustive]
    U64(u64, IntegerEncoder),
    #[non_exhaustive]
    /// 32-bit IEEE 754 single-precision floating-point number
    F32(f32, FloatingEncoder),
    /// 64-bit IEEE 754 double-precision floating-point number
    #[non_exhaustive]
    F64(f64, FloatingEncoder),
}

impl Debug for Encoder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(v) => write!(f, "Bool({})", v),
            Self::I8(v, _) => write!(f, "I8({})", v),
            Self::I16(v, _) => write!(f, "I16({})", v),
            Self::I32(v, _) => write!(f, "I32({})", v),
            Self::I64(v, _) => write!(f, "I64({})", v),
            Self::U64(v, _) => write!(f, "U64({})", v),
            Self::F32(v, _) => write!(f, "F32({})", v),
            Self::F64(v, _) => write!(f, "F64({})", v),
        }
    }
}

impl Encoder {
    /// Encodes a Smithy primitive as a string.
    pub fn encode(&mut self) -> &str {
        match self {
            Encoder::Bool(true) => "true",
            Encoder::Bool(false) => "false",
            Encoder::I8(v, IntegerEncoder(buf)) => buf.format(*v),
            Encoder::I16(v, IntegerEncoder(buf)) => buf.format(*v),
            Encoder::I32(v, IntegerEncoder(buf)) => buf.format(*v),
            Encoder::I64(v, IntegerEncoder(buf)) => buf.format(*v),
            Encoder::U64(v, IntegerEncoder(buf)) => buf.format(*v),
            Encoder::F32(v, FloatingEncoder(buf)) => {
                if v.is_nan() {
                    float::NAN
                } else if *v == f32::INFINITY {
                    float::INFINITY
                } else if *v == f32::NEG_INFINITY {
                    float::NEG_INFINITY
                } else {
                    buf.format_finite(*v)
                }
            }
            Encoder::F64(v, FloatingEncoder(buf)) => {
                if v.is_nan() {
                    float::NAN
                } else if *v == f64::INFINITY {
                    float::INFINITY
                } else if *v == f64::NEG_INFINITY {
                    float::NEG_INFINITY
                } else {
                    buf.format_finite(*v)
                }
            }
        }
    }
}

impl From<bool> for Encoder {
    fn from(input: bool) -> Self {
        Self::Bool(input)
    }
}

impl From<i8> for Encoder {
    fn from(input: i8) -> Self {
        Self::I8(input, Default::default())
    }
}

impl From<i16> for Encoder {
    fn from(input: i16) -> Self {
        Self::I16(input, Default::default())
    }
}

impl From<i32> for Encoder {
    fn from(input: i32) -> Self {
        Self::I32(input, Default::default())
    }
}

impl From<i64> for Encoder {
    fn from(input: i64) -> Self {
        Self::I64(input, Default::default())
    }
}

impl From<u64> for Encoder {
    fn from(input: u64) -> Self {
        Self::U64(input, Default::default())
    }
}

impl From<f32> for Encoder {
    fn from(input: f32) -> Self {
        Self::F32(input, Default::default())
    }
}

impl From<f64> for Encoder {
    fn from(input: f64) -> Self {
        Self::F64(input, Default::default())
    }
}

mod float {
    use std::num::ParseFloatError;

    /// Smithy encoded value for `f64::INFINITY`
    pub(crate) const INFINITY: &str = "Infinity";

    /// Smithy encoded value for `f64::NEG_INFINITY`
    pub(crate) const NEG_INFINITY: &str = "-Infinity";

    /// Smithy encoded value for `f64::NAN`
    pub(crate) const NAN: &str = "NaN";

    /// Parses a Smithy encoded primitive string into an `f32`.
    pub(crate) fn parse_f32(data: &str) -> Result<f32, ParseFloatError> {
        match data {
            INFINITY => Ok(f32::INFINITY),
            NEG_INFINITY => Ok(f32::NEG_INFINITY),
            NAN => Ok(f32::NAN),
            other => other.parse::<f32>(),
        }
    }

    /// Parses a Smithy encoded primitive string into an `f64`.
    pub(crate) fn parse_f64(data: &str) -> Result<f64, ParseFloatError> {
        match data {
            INFINITY => Ok(f64::INFINITY),
            NEG_INFINITY => Ok(f64::NEG_INFINITY),
            NAN => Ok(f64::NAN),
            other => other.parse::<f64>(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::primitive::{Encoder, Parse};

    #[test]
    fn bool_format() {
        assert_eq!(Encoder::from(true).encode(), "true");
        assert_eq!(Encoder::from(false).encode(), "false");
        let err = bool::parse_smithy_primitive("not a boolean").expect_err("should fail");
        assert_eq!(err.0, "bool");
        assert_eq!(bool::parse_smithy_primitive("true"), Ok(true));
        assert_eq!(bool::parse_smithy_primitive("false"), Ok(false));
    }

    #[test]
    fn float_format() {
        assert_eq!(Encoder::from(55_f64).encode(), "55.0");
        assert_eq!(Encoder::from(f64::INFINITY).encode(), "Infinity");
        assert_eq!(Encoder::from(f32::INFINITY).encode(), "Infinity");
        assert_eq!(Encoder::from(f32::NEG_INFINITY).encode(), "-Infinity");
        assert_eq!(Encoder::from(f64::NEG_INFINITY).encode(), "-Infinity");
        assert_eq!(Encoder::from(f32::NAN).encode(), "NaN");
        assert_eq!(Encoder::from(f64::NAN).encode(), "NaN");
    }

    #[test]
    fn float_parse() {
        assert_eq!(f64::parse_smithy_primitive("1234.5"), Ok(1234.5));
        assert!(f64::parse_smithy_primitive("NaN").unwrap().is_nan());
        assert_eq!(
            f64::parse_smithy_primitive("Infinity").unwrap(),
            f64::INFINITY
        );
        assert_eq!(
            f64::parse_smithy_primitive("-Infinity").unwrap(),
            f64::NEG_INFINITY
        );
        assert_eq!(f32::parse_smithy_primitive("1234.5"), Ok(1234.5));
        assert!(f32::parse_smithy_primitive("NaN").unwrap().is_nan());
        assert_eq!(
            f32::parse_smithy_primitive("Infinity").unwrap(),
            f32::INFINITY
        );
        assert_eq!(
            f32::parse_smithy_primitive("-Infinity").unwrap(),
            f32::NEG_INFINITY
        );
    }
}
