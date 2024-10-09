/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_cbor::decode::DeserializeError;
use aws_smithy_cbor::Decoder;
use aws_smithy_runtime_api::http::Headers;
use aws_smithy_types::error::metadata::{Builder as ErrorMetadataBuilder, ErrorMetadata};

// This function is a copy-paste from `json_errors::sanitize_error_code`, therefore the functional
// tests can be viewed in the unit tests there.
// Since this is in the `inlineable` crate, there aren't good modules for housing common utilities
// unless we move this to a Smithy runtime crate.
fn sanitize_error_code(error_code: &str) -> &str {
    // Trim a trailing URL from the error code, which is done by removing the longest suffix
    // beginning with a `:`
    let error_code = match error_code.find(':') {
        Some(idx) => &error_code[..idx],
        None => error_code,
    };

    // Trim a prefixing namespace from the error code, beginning with a `#`
    match error_code.find('#') {
        Some(idx) => &error_code[idx + 1..],
        None => error_code,
    }
}

pub fn parse_error_metadata(
    _response_status: u16,
    _response_headers: &Headers,
    response_body: &[u8],
) -> Result<ErrorMetadataBuilder, DeserializeError> {
    fn error_code_and_message(
        mut builder: ErrorMetadataBuilder,
        decoder: &mut Decoder,
    ) -> Result<ErrorMetadataBuilder, DeserializeError> {
        builder = match decoder.str()?.as_ref() {
            "__type" => {
                let code = decoder.str()?;
                builder.code(sanitize_error_code(&code))
            }
            "message" | "Message" | "errorMessage" => {
                // Silently skip if `message` is not a string. This allows for custom error
                // structures that might use different types for the message field.
                match decoder.str() {
                    Ok(message) => builder.message(message),
                    Err(_) => builder,
                }
            }
            _ => {
                decoder.skip()?;
                builder
            }
        };
        Ok(builder)
    }

    let decoder = &mut Decoder::new(response_body);
    let mut builder = ErrorMetadata::builder();

    match decoder.map()? {
        None => loop {
            match decoder.datatype()? {
                ::aws_smithy_cbor::data::Type::Break => {
                    decoder.skip()?;
                    break;
                }
                _ => {
                    builder = error_code_and_message(builder, decoder)?;
                }
            };
        },
        Some(n) => {
            for _ in 0..n {
                builder = error_code_and_message(builder, decoder)?;
            }
        }
    };

    Ok(builder)
}
