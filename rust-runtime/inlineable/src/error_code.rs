/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use http::header::ToStrError;

pub fn is_error<B>(response: &http::Response<B>) -> bool {
    !response.status().is_success()
}

pub fn error_type_from_header<B>(response: &http::Response<B>) -> Result<Option<&str>, ToStrError> {
    response
        .headers()
        .get("X-Amzn-Errortype")
        .map(|v| v.to_str())
        .transpose()
}

pub fn error_type_from_body(body: &serde_json::Value) -> Option<&str> {
    body.as_object()
        .and_then(|b: &serde_json::Map<String, serde_json::Value>| {
            b.get("code").or_else(|| b.get("__type"))
        })
        .and_then(|v| v.as_str())
}

pub fn sanitize_error_code(error_code: &str) -> &str {
    // Trim a trailing URL from the error code, beginning with a `:`
    let error_code = match error_code.find(':') {
        Some(idx) => &error_code[..idx],
        None => &error_code,
    };

    // Trim a prefixing namespace from the error code, beginning with a `#`
    match error_code.find('#') {
        Some(idx) => &error_code[idx + 1..],
        None => &error_code,
    }
}

#[cfg(test)]
mod test {
    use crate::error_code::{error_type_from_body, sanitize_error_code};
    use serde_json::json;

    #[test]
    fn test_error_type() {
        let error_body = json!({
            "__type": "FooError"
        });
        assert_eq!(error_type_from_body(&error_body), Some("FooError"));
    }

    #[test]
    fn code_takes_priority() {
        let error_body = json!({
            "__type": "FooError",
            "code": "BarError"
        });
        assert_eq!(error_type_from_body(&error_body), Some("BarError"));
    }

    #[test]
    fn sanitize_namespace_and_url() {
        assert_eq!(
            sanitize_error_code("aws.protocoltests.restjson#FooError:http://internal.amazon.com/coral/com.amazon.coral.validate/"),
            "FooError");
    }

    #[test]
    fn sanitize_noop() {
        assert_eq!(sanitize_error_code("FooError"), "FooError");
    }

    #[test]
    fn sanitize_url() {
        assert_eq!(
            sanitize_error_code(
                "FooError:http://internal.amazon.com/coral/com.amazon.coral.validate/"
            ),
            "FooError"
        );
    }

    #[test]
    fn sanitize_namespace() {
        assert_eq!(
            sanitize_error_code("aws.protocoltests.restjson#FooError"),
            "FooError"
        );
    }
}
