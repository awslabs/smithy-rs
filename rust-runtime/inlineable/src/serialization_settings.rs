/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

#![allow(dead_code)]

use aws_smithy_types::config_bag::{Storable, StoreReplace};
use http::header::{HeaderName, CONTENT_LENGTH, CONTENT_TYPE};

/// Configuration for how default protocol headers are serialized
#[derive(Clone, Debug, Default)]
pub(crate) struct HeaderSerializationSettings {
    omit_default_content_length: bool,
    omit_default_content_type: bool,
}

impl HeaderSerializationSettings {
    /// Creates new [`HeaderSerializationSettings`]
    pub(crate) fn new() -> Self {
        Default::default()
    }

    /// Omit the default `Content-Length` header during serialization
    pub(crate) fn omit_default_content_length(self) -> Self {
        Self {
            omit_default_content_length: true,
            ..self
        }
    }

    /// Omit the default `Content-Type` header during serialization
    pub(crate) fn omit_default_content_type(self) -> Self {
        Self {
            omit_default_content_type: true,
            ..self
        }
    }

    /// Returns true if the given default header name should be serialized
    fn include_header(&self, header: &HeaderName) -> bool {
        (!self.omit_default_content_length || header != CONTENT_LENGTH)
            && (!self.omit_default_content_type || header != CONTENT_TYPE)
    }

    /// Sets a default header on the given request builder if it should be serialized
    pub(crate) fn set_default_header(
        &self,
        mut request: http::request::Builder,
        header_name: HeaderName,
        value: &str,
    ) -> http::request::Builder {
        if self.include_header(&header_name) {
            // TODO(hyper1) - revert back to use aws_smithy_http::header::set_request_header_if_absent once codegen is on http_1x types
            if !request
                .headers_ref()
                .map(|map| map.contains_key(&header_name))
                .unwrap_or(false)
            {
                request = request.header(header_name, value)
            }
        }
        request
    }
}

impl Storable for HeaderSerializationSettings {
    type Storer = StoreReplace<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_include_header() {
        let settings = HeaderSerializationSettings::default();
        assert!(settings.include_header(&CONTENT_LENGTH));
        assert!(settings.include_header(&CONTENT_TYPE));

        let settings = HeaderSerializationSettings::default().omit_default_content_length();
        assert!(!settings.include_header(&CONTENT_LENGTH));
        assert!(settings.include_header(&CONTENT_TYPE));

        let settings = HeaderSerializationSettings::default().omit_default_content_type();
        assert!(settings.include_header(&CONTENT_LENGTH));
        assert!(!settings.include_header(&CONTENT_TYPE));

        let settings = HeaderSerializationSettings::default()
            .omit_default_content_type()
            .omit_default_content_length();
        assert!(!settings.include_header(&CONTENT_LENGTH));
        assert!(!settings.include_header(&CONTENT_TYPE));
    }
}
