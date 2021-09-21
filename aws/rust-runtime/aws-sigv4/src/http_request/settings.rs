/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

pub type SigningParams<'a> = crate::SigningParams<'a, SigningSettings>;

#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct SigningSettings {
    /// We assume the URI will be encoded _once_ prior to transmission. Some services
    /// do not decode the path prior to checking the signature, requiring clients to actually
    /// _double-encode_ the URI in creating the canonical request in order to pass a signature check.
    pub uri_encoding: UriEncoding,

    /// Add an additional checksum header
    pub payload_checksum_kind: PayloadChecksumKind,

    /// Where to put the signature
    pub signature_location: SignatureLocation,
}

#[non_exhaustive]
#[derive(Debug, Eq, PartialEq)]
pub enum PayloadChecksumKind {
    /// Add x-amz-checksum-sha256 to the canonical request
    ///
    /// This setting is required for S3
    XAmzSha256,

    /// Do not add an additional header when creating the canonical request
    ///
    /// This is "normal mode" and will work for services other than S3
    NoHeader,
}

#[non_exhaustive]
#[derive(Debug, Eq, PartialEq)]
pub enum UriEncoding {
    /// Re-encode the resulting URL (eg. %30 becomes `%2530)
    Double,

    /// Take the resulting URL as-is
    Single,
}

impl Default for SigningSettings {
    fn default() -> Self {
        Self {
            uri_encoding: UriEncoding::Double,
            payload_checksum_kind: PayloadChecksumKind::NoHeader,
            signature_location: SignatureLocation::Headers,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Eq, PartialEq)]
pub enum SignatureLocation {
    /// Place the signature in the request headers
    Headers,
    /// Place the signature in the request query parameters
    QueryParams,
}
