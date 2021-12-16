/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Rejection response types.
define_rejection! {
    #[status = INTERNAL_SERVER_ERROR]
    #[body = "Cannot have two request body extractors for a single request"]
    /// Rejection type used if you try and extract the request body more than
    /// once.
    pub struct BodyAlreadyExtracted;
}

define_rejection! {
    #[status = INTERNAL_SERVER_ERROR]
    #[body = "Headers taken by other extractor"]
    /// Rejection type used if the headers have been taken by another extractor.
    pub struct HeadersAlreadyExtracted;
}

define_rejection! {
    #[status = BAD_REQUEST]
    #[body = "Request deserialize failed"]
    /// Rejection type used if the request deserialization encountered errors.
    pub struct Deserialize(Error);
}

define_rejection! {
    #[status = INTERNAL_SERVER_ERROR]
    #[body = "Response serialize failed"]
    /// Rejection type used if the response serialization encountered errors.
    pub struct Serialize(Error);
}

define_rejection! {
    #[status = BAD_REQUEST]
    #[body = "Request body does not contain valid UTF-8"]
    /// Rejection type used when buffering the request into a [`String`] if the
    /// body doesn't contain valid UTF-8.
    pub struct InvalidUtf8(Error);
}

define_rejection! {
    // TODO: we probably want to be more specific as the http::Error enum has many variants
    #[status = INTERNAL_SERVER_ERROR]
    #[body = "Error handling HTTP request"]
    /// Rejection type used when there is an error handling the HTTP request.
    pub struct Http(Error);
}

define_rejection! {
    // TODO: we probably want to be more specific as the header parsing can have many variants
    #[status = BAD_REQUEST]
    #[body = "Error parsing headers"]
    /// Rejection type used if the any of the header parsing fails.
    pub struct HeadersParse(Error);
}

define_rejection! {
    #[status = BAD_REQUEST]
    #[body = "Expected `Content-Type: application/json`"]
    /// Rejection type used if the JSON `Content-Type` header is missing.
    pub struct MissingJsonContentType;
}

define_rejection! {
    #[status = BAD_REQUEST]
    #[body = "Expected `Content-Type: application/xml`"]
    /// Rejection type used if the XML `Content-Type` header is missing.
    pub struct MissingXmlContentType;
}

define_rejection! {
    #[status = BAD_REQUEST]
    #[body = "Expected query string in URI but none found"]
    /// Rejection type used if the URI has no query string and we need to deserialize data from it.
    pub struct MissingQueryString;
}

define_rejection! {
    #[status = BAD_REQUEST]
    #[body = "Failed to parse request MIME type"]
    /// Rejection type used if the MIME type parsing failed.
    pub struct MimeParsingFailed;
}

define_rejection! {
    #[status = INTERNAL_SERVER_ERROR]
    #[body = "Extensions taken by other extractor"]
    /// Rejection used if the request extension has been taken by another
    /// extractor.
    pub struct ExtensionsAlreadyExtracted;
}

define_rejection! {
    #[status = INTERNAL_SERVER_ERROR]
    #[body = "Missing request extension"]
    /// Rejection type for [`Extension`](super::Extension) if an expected
    /// request extension was not found.
    pub struct MissingExtension(Error);
}

composite_rejection! {
    /// Rejection used for `Content-Type` errors such as missing `Content-Type`
    /// header, MIME parse issues, etc.
    pub enum ContentTypeRejection {
        MissingJsonContentType,
        MissingXmlContentType,
        MimeParsingFailed,
    }
}

composite_rejection! {
    /// Rejection used for [`Extension`](super::Extension).
    ///
    /// Contains one variant for each way the [`Extension`](super::Extension) extractor
    /// can fail.
    pub enum ExtensionHandlingRejection {
        MissingExtension,
        ExtensionsAlreadyExtracted,
    }
}

composite_rejection! {
    /// General rejection type used by `smithy-rs` auto-generated extractors and responders.
    ///
    /// Contains one variant for each way extracting and responding can fail.
    ///
    /// This rejection type also aggregates all the errors that come from other `smithy-rs` runtime
    /// crates, allowing a nice integration with serialization, deserialization, and builder types
    /// generated by the codegen.
    pub enum SmithyRejection {
        Serialize,
        Deserialize,
        InvalidUtf8,
        Http,
        HeadersParse,
        ContentTypeRejection,
        BodyAlreadyExtracted,
        HeadersAlreadyExtracted,
        ExtensionsAlreadyExtracted,
        MissingQueryString,
    }
}

impl From<aws_smithy_json::deserialize::Error> for SmithyRejection {
    fn from(err: aws_smithy_json::deserialize::Error) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<aws_smithy_xml::decode::XmlError> for SmithyRejection {
    fn from(err: aws_smithy_xml::decode::XmlError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<aws_smithy_http::operation::BuildError> for SmithyRejection {
    fn from(err: aws_smithy_http::operation::BuildError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<std::num::ParseIntError> for SmithyRejection {
    fn from(err: std::num::ParseIntError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<std::num::ParseFloatError> for SmithyRejection {
    fn from(err: std::num::ParseFloatError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<std::str::ParseBoolError> for SmithyRejection {
    fn from(err: std::str::ParseBoolError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<aws_smithy_types::date_time::DateTimeParseError> for SmithyRejection {
    fn from(err: aws_smithy_types::date_time::DateTimeParseError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<aws_smithy_types::primitive::PrimitiveParseError> for SmithyRejection {
    fn from(err: aws_smithy_types::primitive::PrimitiveParseError) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}

impl From<aws_smithy_http::operation::SerializationError> for SmithyRejection {
    fn from(err: aws_smithy_http::operation::SerializationError) -> Self {
        SmithyRejection::Serialize(Serialize::from_err(err))
    }
}

impl From<std::str::Utf8Error> for SmithyRejection {
    fn from(err: std::str::Utf8Error) -> Self {
        SmithyRejection::InvalidUtf8(InvalidUtf8::from_err(err))
    }
}

impl From<http::Error> for SmithyRejection {
    fn from(err: http::Error) -> Self {
        SmithyRejection::Http(Http::from_err(err))
    }
}

impl From<hyper::Error> for SmithyRejection {
    fn from(err: hyper::Error) -> Self {
        SmithyRejection::Http(Http::from_err(err))
    }
}

impl From<aws_smithy_http::header::ParseError> for SmithyRejection {
    fn from(err: aws_smithy_http::header::ParseError) -> Self {
        SmithyRejection::HeadersParse(HeadersParse::from_err(err))
    }
}

impl From<serde_urlencoded::de::Error> for SmithyRejection {
    fn from(err: serde_urlencoded::de::Error) -> Self {
        SmithyRejection::Deserialize(Deserialize::from_err(err))
    }
}
