/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

#![allow(dead_code)]

//! Interceptors for modifying requests and responses for the purposes of checksum calculation and validation

use aws_http::content_encoding::{AwsChunkedBody, AwsChunkedBodyOptions};
use aws_runtime::auth::sigv4::SigV4OperationSigningConfig;
use aws_sigv4::http_request::SignableBody;
use aws_smithy_checksums::ChecksumAlgorithm;
use aws_smithy_checksums::{body::calculate, http::HttpChecksum};
use aws_smithy_http::body::{BoxBody, SdkBody};
use aws_smithy_http::operation::error::BuildError;
use aws_smithy_runtime_api::client::interceptors::context::Input;
use aws_smithy_runtime_api::client::interceptors::{
    BeforeSerializationInterceptorContextRef, BeforeTransmitInterceptorContextMut, BoxError,
    Interceptor,
};
use aws_smithy_types::config_bag::{ConfigBag, Layer, Storable, StoreReplace};
use http::HeaderValue;
use http_body::Body;
use std::{fmt, mem};

/// Errors related to constructing checksum-validated HTTP requests
#[derive(Debug)]
pub(crate) enum Error {
    /// Only request bodies with a known size can be checksum validated
    UnsizedRequestBody,
    ChecksumHeadersAreUnsupportedForStreamingBody,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsizedRequestBody => write!(
                f,
                "Only request bodies with a known size can be checksum validated."
            ),
            Self::ChecksumHeadersAreUnsupportedForStreamingBody => write!(
                f,
                "Checksum header insertion is only supported for non-streaming HTTP bodies. \
                   To checksum validate a streaming body, the checksums must be sent as trailers."
            ),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug)]
struct RequestChecksumInterceptorState(ChecksumAlgorithm);
impl Storable for RequestChecksumInterceptorState {
    type Storer = StoreReplace<Self>;
}

pub(crate) struct RequestChecksumInterceptor<AP> {
    algorithm_provider: AP,
}

impl<AP> fmt::Debug for RequestChecksumInterceptor<AP> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestChecksumInterceptor").finish()
    }
}

impl<AP> RequestChecksumInterceptor<AP> {
    pub(crate) fn new(algorithm_provider: AP) -> Self {
        Self { algorithm_provider }
    }
}

impl<AP> Interceptor for RequestChecksumInterceptor<AP>
where
    AP: Fn(&Input) -> Result<Option<ChecksumAlgorithm>, BoxError>,
{
    fn read_before_serialization(
        &self,
        context: &BeforeSerializationInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if let Some(checksum_algorithm) = (self.algorithm_provider)(context.input())? {
            let mut layer = Layer::new("RequestChecksumInterceptor");
            layer.store_put(RequestChecksumInterceptorState(checksum_algorithm));
            cfg.push_layer(layer);
        }

        Ok(())
    }

    /// Calculate a checksum and modify the request to include the checksum as a header
    /// (for in-memory request bodies) or a trailer (for streaming request bodies).
    /// Streaming bodies must be sized or this will return an error.
    fn modify_before_retry_loop(
        &self,
        context: &mut BeforeTransmitInterceptorContextMut<'_>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if let Some(state) = cfg.load::<RequestChecksumInterceptorState>() {
            let checksum_algorithm = state.0;
            let request = context.request_mut();
            add_checksum_for_request_body(request, checksum_algorithm, cfg)?;
        }

        Ok(())
    }
}

fn add_checksum_for_request_body(
    request: &mut http::request::Request<SdkBody>,
    checksum_algorithm: ChecksumAlgorithm,
    cfg: &mut ConfigBag,
) -> Result<(), BoxError> {
    match request.body().bytes() {
        // Body is in-memory: read it and insert the checksum as a header.
        Some(data) => {
            tracing::debug!("applying {checksum_algorithm:?} of the request body as a header");
            let mut checksum = checksum_algorithm.into_impl();
            checksum.update(data);

            request
                .headers_mut()
                .insert(checksum.header_name(), checksum.header_value());
        }
        // Body is streaming: wrap the body so it will emit a checksum as a trailer.
        None => {
            tracing::debug!("applying {checksum_algorithm:?} of the request body as a trailer");
            if let Some(mut signing_config) = cfg.get::<SigV4OperationSigningConfig>().cloned() {
                signing_config.signing_options.payload_override =
                    Some(SignableBody::StreamingUnsignedPayloadTrailer);

                let mut layer = Layer::new("http_body_checksum_sigv4_payload_override");
                layer.put(signing_config);
                cfg.push_layer(layer);
            }
            wrap_streaming_request_body_in_checksum_calculating_body(request, checksum_algorithm)?;
        }
    }
    Ok(())
}

fn wrap_streaming_request_body_in_checksum_calculating_body(
    request: &mut http::request::Request<SdkBody>,
    checksum_algorithm: ChecksumAlgorithm,
) -> Result<(), BuildError> {
    let original_body_size = request
        .body()
        .size_hint()
        .exact()
        .ok_or_else(|| BuildError::other(Error::UnsizedRequestBody))?;

    let mut body = {
        let body = mem::replace(request.body_mut(), SdkBody::taken());

        body.map(move |body| {
            let checksum = checksum_algorithm.into_impl();
            let trailer_len = HttpChecksum::size(checksum.as_ref());
            let body = calculate::ChecksumBody::new(body, checksum);
            let aws_chunked_body_options =
                AwsChunkedBodyOptions::new(original_body_size, vec![trailer_len]);

            let body = AwsChunkedBody::new(body, aws_chunked_body_options);

            SdkBody::from_dyn(BoxBody::new(body))
        })
    };

    let encoded_content_length = body
        .size_hint()
        .exact()
        .ok_or_else(|| BuildError::other(Error::UnsizedRequestBody))?;

    let headers = request.headers_mut();

    headers.insert(
        http::header::HeaderName::from_static("x-amz-trailer"),
        // Convert into a `HeaderName` and then into a `HeaderValue`
        http::header::HeaderName::from(checksum_algorithm).into(),
    );

    headers.insert(
        http::header::CONTENT_LENGTH,
        HeaderValue::from(encoded_content_length),
    );
    headers.insert(
        http::header::HeaderName::from_static("x-amz-decoded-content-length"),
        HeaderValue::from(original_body_size),
    );
    headers.insert(
        http::header::CONTENT_ENCODING,
        HeaderValue::from_str(aws_http::content_encoding::header_value::AWS_CHUNKED)
            .map_err(BuildError::other)
            .expect("\"aws-chunked\" will always be a valid HeaderValue"),
    );

    mem::swap(request.body_mut(), &mut body);

    Ok(())
}

/// Given an `SdkBody`, a `aws_smithy_checksums::ChecksumAlgorithm`, and a pre-calculated checksum,
/// return an `SdkBody` where the body will processed with the checksum algorithm and checked
/// against the pre-calculated checksum.
pub(crate) fn wrap_body_with_checksum_validator(
    body: SdkBody,
    checksum_algorithm: ChecksumAlgorithm,
    precalculated_checksum: bytes::Bytes,
) -> SdkBody {
    use aws_smithy_checksums::body::validate;

    body.map(move |body| {
        SdkBody::from_dyn(BoxBody::new(validate::ChecksumBody::new(
            body,
            checksum_algorithm.into_impl(),
            precalculated_checksum.clone(),
        )))
    })
}

/// Given a `HeaderMap`, extract any checksum included in the headers as `Some(Bytes)`.
/// If no checksum header is set, return `None`. If multiple checksum headers are set, the one that
/// is fastest to compute will be chosen.
pub(crate) fn check_headers_for_precalculated_checksum(
    headers: &http::HeaderMap<HeaderValue>,
    response_algorithms: &[&str],
) -> Option<(ChecksumAlgorithm, bytes::Bytes)> {
    let checksum_algorithms_to_check =
        aws_smithy_checksums::http::CHECKSUM_ALGORITHMS_IN_PRIORITY_ORDER
            .into_iter()
            // Process list of algorithms, from fastest to slowest, that may have been used to checksum
            // the response body, ignoring any that aren't marked as supported algorithms by the model.
            .flat_map(|algo| {
                // For loop is necessary b/c the compiler doesn't infer the correct lifetimes for iter().find()
                for res_algo in response_algorithms {
                    if algo.eq_ignore_ascii_case(res_algo) {
                        return Some(algo);
                    }
                }

                None
            });

    for checksum_algorithm in checksum_algorithms_to_check {
        let checksum_algorithm: ChecksumAlgorithm = checksum_algorithm.parse().expect(
            "CHECKSUM_ALGORITHMS_IN_PRIORITY_ORDER only contains valid checksum algorithm names",
        );
        if let Some(precalculated_checksum) =
            headers.get(http::HeaderName::from(checksum_algorithm))
        {
            let base64_encoded_precalculated_checksum = precalculated_checksum
                .to_str()
                .expect("base64 uses ASCII characters");

            // S3 needs special handling for checksums of objects uploaded with `MultiPartUpload`.
            if is_part_level_checksum(base64_encoded_precalculated_checksum) {
                tracing::warn!(
                      more_info = "See https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity.html#large-object-checksums for more information.",
                      "This checksum is a part-level checksum which can't be validated by the Rust SDK. Disable checksum validation for this request to fix this warning.",
                  );

                return None;
            }

            let precalculated_checksum = match aws_smithy_types::base64::decode(
                base64_encoded_precalculated_checksum,
            ) {
                Ok(decoded_checksum) => decoded_checksum.into(),
                Err(_) => {
                    tracing::error!("Checksum received from server could not be base64 decoded. No checksum validation will be performed.");
                    return None;
                }
            };

            return Some((checksum_algorithm, precalculated_checksum));
        }
    }

    None
}

fn is_part_level_checksum(checksum: &str) -> bool {
    let mut found_number = false;
    let mut found_dash = false;

    for ch in checksum.chars().rev() {
        // this could be bad
        if ch.is_ascii_digit() {
            found_number = true;
            continue;
        }

        // Yup, it's a part-level checksum
        if ch == '-' {
            if found_dash {
                // Found a second dash?? This isn't a part-level checksum.
                return false;
            }

            found_dash = true;
            continue;
        }

        break;
    }

    found_number && found_dash
}

#[cfg(test)]
mod tests {
    use super::wrap_body_with_checksum_validator;
    use crate::http_body_checksum::is_part_level_checksum;
    use aws_smithy_checksums::ChecksumAlgorithm;
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_http::byte_stream::ByteStream;
    use aws_smithy_types::error::display::DisplayErrorContext;
    use bytes::{Bytes, BytesMut};
    use http_body::Body;
    use std::sync::Once;
    use tempfile::NamedTempFile;

    static INIT_LOGGER: Once = Once::new();
    fn init_logger() {
        INIT_LOGGER.call_once(|| {
            tracing_subscriber::fmt::init();
        });
    }

    #[tokio::test]
    async fn test_checksum_body_is_retryable() {
        let input_text = "Hello world";
        let precalculated_checksum = Bytes::from_static(&[0x8b, 0xd6, 0x9e, 0x52]);
        let body = SdkBody::retryable(move || SdkBody::from(input_text));

        // ensure original SdkBody is retryable
        assert!(body.try_clone().is_some());

        let body = body.map(move |sdk_body| {
            let checksum_algorithm: ChecksumAlgorithm = "crc32".parse().unwrap();
            wrap_body_with_checksum_validator(
                sdk_body,
                checksum_algorithm,
                precalculated_checksum.clone(),
            )
        });

        // ensure wrapped SdkBody is retryable
        let mut body = body.try_clone().expect("body is retryable");

        let mut validated_body = BytesMut::new();

        loop {
            match body.data().await {
                Some(Ok(data)) => validated_body.extend_from_slice(&data),
                Some(Err(err)) => panic!("{}", err),
                None => {
                    break;
                }
            }
        }

        let body = std::str::from_utf8(&validated_body).unwrap();

        // ensure that the wrapped body passes checksum validation
        assert_eq!(input_text, body);
    }

    #[tokio::test]
    async fn test_checksum_body_from_file_is_retryable() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        let checksum_algorithm: ChecksumAlgorithm = "crc32c".parse().unwrap();
        let mut crc32c_checksum = checksum_algorithm.into_impl();

        for i in 0..10000 {
            let line = format!("This is a large file created for testing purposes {}", i);
            file.as_file_mut().write_all(line.as_bytes()).unwrap();
            crc32c_checksum.update(line.as_bytes());
        }

        let body = ByteStream::read_from()
            .path(&file)
            .buffer_size(1024)
            .build()
            .await
            .unwrap();

        let precalculated_checksum = crc32c_checksum.finalize();
        let expected_checksum = precalculated_checksum.clone();

        let body = body.map(move |sdk_body| {
            wrap_body_with_checksum_validator(
                sdk_body,
                checksum_algorithm,
                precalculated_checksum.clone(),
            )
        });

        // ensure wrapped SdkBody is retryable
        let mut body = body.into_inner().try_clone().expect("body is retryable");

        let mut validated_body = BytesMut::new();

        // If this loop completes, then it means the body's checksum was valid, but let's calculate
        // a checksum again just in case.
        let mut redundant_crc32c_checksum = checksum_algorithm.into_impl();
        loop {
            match body.data().await {
                Some(Ok(data)) => {
                    redundant_crc32c_checksum.update(&data);
                    validated_body.extend_from_slice(&data);
                }
                Some(Err(err)) => panic!("{}", err),
                None => {
                    break;
                }
            }
        }

        let actual_checksum = redundant_crc32c_checksum.finalize();
        assert_eq!(expected_checksum, actual_checksum);

        // Ensure the file's checksum isn't the same as an empty checksum. This way, we'll know that
        // data was actually processed.
        let unexpected_checksum = checksum_algorithm.into_impl().finalize();
        assert_ne!(unexpected_checksum, actual_checksum);
    }

    #[tokio::test]
    async fn test_build_checksum_validated_body_works() {
        init_logger();

        let checksum_algorithm = "crc32".parse().unwrap();
        let input_text = "Hello world";
        let precalculated_checksum = Bytes::from_static(&[0x8b, 0xd6, 0x9e, 0x52]);
        let body = ByteStream::new(SdkBody::from(input_text));

        let body = body.map(move |sdk_body| {
            wrap_body_with_checksum_validator(
                sdk_body,
                checksum_algorithm,
                precalculated_checksum.clone(),
            )
        });

        let mut validated_body = Vec::new();
        if let Err(e) = tokio::io::copy(&mut body.into_async_read(), &mut validated_body).await {
            tracing::error!("{}", DisplayErrorContext(&e));
            panic!("checksum validation has failed");
        };
        let body = std::str::from_utf8(&validated_body).unwrap();

        assert_eq!(input_text, body);
    }

    #[test]
    fn test_is_multipart_object_checksum() {
        // These ARE NOT part-level checksums
        assert!(!is_part_level_checksum("abcd"));
        assert!(!is_part_level_checksum("abcd="));
        assert!(!is_part_level_checksum("abcd=="));
        assert!(!is_part_level_checksum("1234"));
        assert!(!is_part_level_checksum("1234="));
        assert!(!is_part_level_checksum("1234=="));
        // These ARE part-level checksums
        assert!(is_part_level_checksum("abcd-1"));
        assert!(is_part_level_checksum("abcd=-12"));
        assert!(is_part_level_checksum("abcd12-134"));
        assert!(is_part_level_checksum("abcd==-10000"));
        // These are gibberish and shouldn't be regarded as a part-level checksum
        assert!(!is_part_level_checksum(""));
        assert!(!is_part_level_checksum("Spaces? In my header values?"));
        assert!(!is_part_level_checksum("abcd==-134!#{!#"));
        assert!(!is_part_level_checksum("abcd==-"));
        assert!(!is_part_level_checksum("abcd==--11"));
        assert!(!is_part_level_checksum("abcd==-AA"));
    }
}
