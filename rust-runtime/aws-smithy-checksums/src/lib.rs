/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

/* Automatically managed default lints */
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
/* End of automatically managed default lints */
#![allow(clippy::derive_partial_eq_without_eq)]
#![warn(
    // missing_docs,
    rustdoc::missing_crate_level_docs,
    unreachable_pub,
    rust_2018_idioms
)]

//! Checksum calculation and verification callbacks.

use crate::error::UnknownChecksumAlgorithmError;
use crate::error::{
    UnknownRequestChecksumCalculationError, UnknownResponseChecksumValidationError,
};
use aws_smithy_types::config_bag::{Storable, StoreReplace};
use bytes::Bytes;
use std::str::FromStr;

pub mod body;
pub mod error;
pub mod http;

// Valid checksum algorithm names
pub const CRC_32_NAME: &str = "crc32";
pub const CRC_32_C_NAME: &str = "crc32c";
pub const SHA_1_NAME: &str = "sha1";
pub const SHA_256_NAME: &str = "sha256";
pub const MD5_NAME: &str = "md5";

/// We only support checksum calculation and validation for these checksum algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChecksumAlgorithm {
    Crc32,
    Crc32c,
    #[deprecated]
    Md5,
    Sha1,
    Sha256,
}

impl FromStr for ChecksumAlgorithm {
    type Err = UnknownChecksumAlgorithmError;

    /// Create a new `ChecksumAlgorithm` from an algorithm name. Valid algorithm names are:
    /// - "crc32"
    /// - "crc32c"
    /// - "sha1"
    /// - "sha256"
    ///
    /// Passing an invalid name will return an error.
    fn from_str(checksum_algorithm: &str) -> Result<Self, Self::Err> {
        if checksum_algorithm.eq_ignore_ascii_case(CRC_32_NAME) {
            Ok(Self::Crc32)
        } else if checksum_algorithm.eq_ignore_ascii_case(CRC_32_C_NAME) {
            Ok(Self::Crc32c)
        } else if checksum_algorithm.eq_ignore_ascii_case(SHA_1_NAME) {
            Ok(Self::Sha1)
        } else if checksum_algorithm.eq_ignore_ascii_case(SHA_256_NAME) {
            Ok(Self::Sha256)
        } else if checksum_algorithm.eq_ignore_ascii_case(MD5_NAME) {
            // MD5 is now an alias for the default Crc32 since it is deprecated
            Ok(Self::Crc32)
        } else {
            Err(UnknownChecksumAlgorithmError::new(checksum_algorithm))
        }
    }
}

impl ChecksumAlgorithm {
    /// Return the `HttpChecksum` implementor for this algorithm
    pub fn into_impl(self) -> Box<dyn http::HttpChecksum> {
        match self {
            Self::Crc32 => Box::<Crc32>::default(),
            Self::Crc32c => Box::<Crc32c>::default(),
            #[allow(deprecated)]
            Self::Md5 => Box::<Crc32>::default(),
            Self::Sha1 => Box::<Sha1>::default(),
            Self::Sha256 => Box::<Sha256>::default(),
        }
    }

    /// Return the name of this algorithm in string form
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Crc32 => CRC_32_NAME,
            Self::Crc32c => CRC_32_C_NAME,
            #[allow(deprecated)]
            Self::Md5 => MD5_NAME,
            Self::Sha1 => SHA_1_NAME,
            Self::Sha256 => SHA_256_NAME,
        }
    }
}

// Valid names for RequestChecksumCalculation and ResponseChecksumValidation
pub const WHEN_SUPPORTED: &str = "when_supported";
pub const WHEN_REQUIRED: &str = "when_required";

/// Determines when a checksum will be calculated for request payloads. Values are:
/// * [RequestChecksumCalculation::WhenSupported] - (default) When set, a checksum will be
/// calculated for all request payloads of operations modeled with the
/// `httpChecksum` trait where `requestChecksumRequired` is `true` and/or a
/// `requestAlgorithmMember` is modeled.
/// * [RequestChecksumCalculation::WhenRequired] - When set, a checksum will only be calculated for
/// request payloads of operations modeled with the  `httpChecksum` trait where
/// `requestChecksumRequired` is `true` or where a requestAlgorithmMember
/// is modeled and supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RequestChecksumCalculation {
    WhenSupported,
    WhenRequired,
}

impl Storable for RequestChecksumCalculation {
    type Storer = StoreReplace<Self>;
}

impl FromStr for RequestChecksumCalculation {
    type Err = UnknownRequestChecksumCalculationError;

    fn from_str(request_checksum_calculation: &str) -> Result<Self, Self::Err> {
        if request_checksum_calculation.eq_ignore_ascii_case(WHEN_SUPPORTED) {
            Ok(Self::WhenSupported)
        } else if request_checksum_calculation.eq_ignore_ascii_case(WHEN_REQUIRED) {
            Ok(Self::WhenRequired)
        } else {
            Err(UnknownRequestChecksumCalculationError::new(
                request_checksum_calculation,
            ))
        }
    }
}

/// Determines when checksum validation will be performed on response payloads. Values are:
/// * [ResponseChecksumValidation::WhenSupported] - (default) When set, checksum validation is performed on all
/// response payloads of operations modeled with the `httpChecksum` trait where
/// `responseAlgorithms` is modeled, except when no modeled checksum algorithms
/// are supported.
/// * [ResponseChecksumValidation::WhenRequired] - When set, checksum validation is not performed on
/// response payloads of operations unless the checksum algorithm is supported and
/// the `requestValidationModeMember` member is set to `ENABLED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResponseChecksumValidation {
    WhenSupported,
    WhenRequired,
}

impl Storable for ResponseChecksumValidation {
    type Storer = StoreReplace<Self>;
}

impl FromStr for ResponseChecksumValidation {
    type Err = UnknownResponseChecksumValidationError;

    fn from_str(response_checksum_validation: &str) -> Result<Self, Self::Err> {
        if response_checksum_validation.eq_ignore_ascii_case(WHEN_SUPPORTED) {
            Ok(Self::WhenSupported)
        } else if response_checksum_validation.eq_ignore_ascii_case(WHEN_REQUIRED) {
            Ok(Self::WhenRequired)
        } else {
            Err(UnknownResponseChecksumValidationError::new(
                response_checksum_validation,
            ))
        }
    }
}

/// Types implementing this trait can calculate checksums.
///
/// Checksum algorithms are used to validate the integrity of data. Structs that implement this trait
/// can be used as checksum calculators. This trait requires Send + Sync because these checksums are
/// often used in a threaded context.
pub trait Checksum: Send + Sync {
    /// Given a slice of bytes, update this checksum's internal state.
    fn update(&mut self, bytes: &[u8]);
    /// "Finalize" this checksum, returning the calculated value as `Bytes` or an error that
    /// occurred during checksum calculation.
    ///
    /// _HINT: To print this value in a human-readable hexadecimal format, you can use Rust's
    /// builtin [formatter]._
    ///
    /// [formatter]: https://doc.rust-lang.org/std/fmt/trait.UpperHex.html
    fn finalize(self: Box<Self>) -> Bytes;
    /// Return the size of this checksum algorithms resulting checksum, in bytes.
    ///
    /// For example, the CRC32 checksum algorithm calculates a 32 bit checksum, so a CRC32 checksum
    /// struct implementing this trait method would return `4`.
    fn size(&self) -> u64;
}

#[derive(Debug, Default)]
struct Crc32 {
    hasher: crc32fast::Hasher,
}

impl Crc32 {
    fn update(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn finalize(self) -> Bytes {
        Bytes::copy_from_slice(self.hasher.finalize().to_be_bytes().as_slice())
    }

    // Size of the checksum in bytes
    fn size() -> u64 {
        4
    }
}

impl Checksum for Crc32 {
    fn update(&mut self, bytes: &[u8]) {
        Self::update(self, bytes)
    }
    fn finalize(self: Box<Self>) -> Bytes {
        Self::finalize(*self)
    }
    fn size(&self) -> u64 {
        Self::size()
    }
}

#[derive(Debug, Default)]
struct Crc32c {
    state: Option<u32>,
}

impl Crc32c {
    fn update(&mut self, bytes: &[u8]) {
        self.state = match self.state {
            Some(crc) => Some(crc32c::crc32c_append(crc, bytes)),
            None => Some(crc32c::crc32c(bytes)),
        };
    }

    fn finalize(self) -> Bytes {
        Bytes::copy_from_slice(self.state.unwrap_or_default().to_be_bytes().as_slice())
    }

    // Size of the checksum in bytes
    fn size() -> u64 {
        4
    }
}

impl Checksum for Crc32c {
    fn update(&mut self, bytes: &[u8]) {
        Self::update(self, bytes)
    }
    fn finalize(self: Box<Self>) -> Bytes {
        Self::finalize(*self)
    }
    fn size(&self) -> u64 {
        Self::size()
    }
}

#[derive(Debug, Default)]
struct Sha1 {
    hasher: sha1::Sha1,
}

impl Sha1 {
    fn update(&mut self, bytes: &[u8]) {
        use sha1::Digest;
        self.hasher.update(bytes);
    }

    fn finalize(self) -> Bytes {
        use sha1::Digest;
        Bytes::copy_from_slice(self.hasher.finalize().as_slice())
    }

    // Size of the checksum in bytes
    fn size() -> u64 {
        use sha1::Digest;
        sha1::Sha1::output_size() as u64
    }
}

impl Checksum for Sha1 {
    fn update(&mut self, bytes: &[u8]) {
        Self::update(self, bytes)
    }

    fn finalize(self: Box<Self>) -> Bytes {
        Self::finalize(*self)
    }
    fn size(&self) -> u64 {
        Self::size()
    }
}

#[derive(Debug, Default)]
struct Sha256 {
    hasher: sha2::Sha256,
}

impl Sha256 {
    fn update(&mut self, bytes: &[u8]) {
        use sha2::Digest;
        self.hasher.update(bytes);
    }

    fn finalize(self) -> Bytes {
        use sha2::Digest;
        Bytes::copy_from_slice(self.hasher.finalize().as_slice())
    }

    // Size of the checksum in bytes
    fn size() -> u64 {
        use sha2::Digest;
        sha2::Sha256::output_size() as u64
    }
}

impl Checksum for Sha256 {
    fn update(&mut self, bytes: &[u8]) {
        Self::update(self, bytes);
    }
    fn finalize(self: Box<Self>) -> Bytes {
        Self::finalize(*self)
    }
    fn size(&self) -> u64 {
        Self::size()
    }
}

#[derive(Debug, Default)]
struct Md5 {
    hasher: md5::Md5,
}

impl Md5 {
    fn update(&mut self, bytes: &[u8]) {
        use md5::Digest;
        self.hasher.update(bytes);
    }

    fn finalize(self) -> Bytes {
        use md5::Digest;
        Bytes::copy_from_slice(self.hasher.finalize().as_slice())
    }

    // Size of the checksum in bytes
    fn size() -> u64 {
        use md5::Digest;
        md5::Md5::output_size() as u64
    }
}

impl Checksum for Md5 {
    fn update(&mut self, bytes: &[u8]) {
        Self::update(self, bytes)
    }
    fn finalize(self: Box<Self>) -> Bytes {
        Self::finalize(*self)
    }
    fn size(&self) -> u64 {
        Self::size()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        http::{
            CRC_32_C_HEADER_NAME, CRC_32_HEADER_NAME, MD5_HEADER_NAME, SHA_1_HEADER_NAME,
            SHA_256_HEADER_NAME,
        },
        Crc32, Crc32c, Md5, Sha1, Sha256,
    };

    use crate::http::HttpChecksum;
    use crate::ChecksumAlgorithm;
    use aws_smithy_types::base64;
    use http::HeaderValue;
    use pretty_assertions::assert_eq;
    use std::fmt::Write;

    const TEST_DATA: &str = r#"test data"#;

    fn base64_encoded_checksum_to_hex_string(header_value: &HeaderValue) -> String {
        let decoded_checksum = base64::decode(header_value.to_str().unwrap()).unwrap();
        let decoded_checksum = decoded_checksum
            .into_iter()
            .fold(String::new(), |mut acc, byte| {
                write!(acc, "{byte:02X?}").expect("string will always be writeable");
                acc
            });

        format!("0x{}", decoded_checksum)
    }

    #[test]
    fn test_crc32_checksum() {
        let mut checksum = Crc32::default();
        checksum.update(TEST_DATA.as_bytes());
        let checksum_result = Box::new(checksum).headers();
        let encoded_checksum = checksum_result.get(CRC_32_HEADER_NAME).unwrap();
        let decoded_checksum = base64_encoded_checksum_to_hex_string(encoded_checksum);

        let expected_checksum = "0xD308AEB2";

        assert_eq!(decoded_checksum, expected_checksum);
    }

    // TODO(https://github.com/zowens/crc32c/issues/34)
    // TODO(https://github.com/smithy-lang/smithy-rs/issues/1857)
    #[cfg(not(any(target_arch = "powerpc", target_arch = "powerpc64")))]
    #[test]
    fn test_crc32c_checksum() {
        let mut checksum = Crc32c::default();
        checksum.update(TEST_DATA.as_bytes());
        let checksum_result = Box::new(checksum).headers();
        let encoded_checksum = checksum_result.get(CRC_32_C_HEADER_NAME).unwrap();
        let decoded_checksum = base64_encoded_checksum_to_hex_string(encoded_checksum);

        let expected_checksum = "0x3379B4CA";

        assert_eq!(decoded_checksum, expected_checksum);
    }

    #[test]
    fn test_sha1_checksum() {
        let mut checksum = Sha1::default();
        checksum.update(TEST_DATA.as_bytes());
        let checksum_result = Box::new(checksum).headers();
        let encoded_checksum = checksum_result.get(SHA_1_HEADER_NAME).unwrap();
        let decoded_checksum = base64_encoded_checksum_to_hex_string(encoded_checksum);

        let expected_checksum = "0xF48DD853820860816C75D54D0F584DC863327A7C";

        assert_eq!(decoded_checksum, expected_checksum);
    }

    #[test]
    fn test_sha256_checksum() {
        let mut checksum = Sha256::default();
        checksum.update(TEST_DATA.as_bytes());
        let checksum_result = Box::new(checksum).headers();
        let encoded_checksum = checksum_result.get(SHA_256_HEADER_NAME).unwrap();
        let decoded_checksum = base64_encoded_checksum_to_hex_string(encoded_checksum);

        let expected_checksum =
            "0x916F0027A575074CE72A331777C3478D6513F786A591BD892DA1A577BF2335F9";

        assert_eq!(decoded_checksum, expected_checksum);
    }

    #[test]
    fn test_md5_checksum() {
        let mut checksum = Md5::default();
        checksum.update(TEST_DATA.as_bytes());
        let checksum_result = Box::new(checksum).headers();
        let encoded_checksum = checksum_result.get(MD5_HEADER_NAME).unwrap();
        let decoded_checksum = base64_encoded_checksum_to_hex_string(encoded_checksum);

        let expected_checksum = "0xEB733A00C0C9D336E65691A37AB54293";

        assert_eq!(decoded_checksum, expected_checksum);
    }

    #[test]
    fn test_checksum_algorithm_returns_error_for_unknown() {
        let error = "some invalid checksum algorithm"
            .parse::<ChecksumAlgorithm>()
            .expect_err("it should error");
        assert_eq!(
            "some invalid checksum algorithm",
            error.checksum_algorithm()
        );
    }
}
