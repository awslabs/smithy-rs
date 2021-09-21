/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use super::{PayloadChecksumKind, SignatureLocation};
use crate::http_request::canonical_request::{
    CanonicalRequest, StringToSign, HMAC_256, X_AMZ_CONTENT_SHA_256, X_AMZ_DATE,
    X_AMZ_SECURITY_TOKEN,
};
use crate::http_request::query_writer::QueryWriter;
use crate::http_request::SigningParams;
use crate::sign::{calculate_signature, generate_signing_key, sha256_hex_string};
use crate::SigningOutput;
use http::header::HeaderValue;
use http::{HeaderMap, Method, Uri};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::error::Error as StdError;
use std::str;

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

#[derive(Debug)]
#[non_exhaustive]
pub struct SignableRequest<'a> {
    method: &'a Method,
    uri: &'a Uri,
    headers: &'a HeaderMap<HeaderValue>,
    body: SignableBody<'a>,
}

impl<'a> SignableRequest<'a> {
    pub fn new(
        method: &'a Method,
        uri: &'a Uri,
        headers: &'a HeaderMap<HeaderValue>,
        body: SignableBody<'a>,
    ) -> Self {
        Self {
            method,
            uri,
            headers,
            body,
        }
    }

    pub fn from_http<'b, B>(request: &'b http::Request<B>) -> SignableRequest<'b>
    where
        B: 'b,
        B: AsRef<[u8]>,
    {
        SignableRequest::new(
            request.method(),
            request.uri(),
            request.headers(),
            SignableBody::Bytes(request.body().as_ref()),
        )
    }

    pub fn uri(&self) -> &Uri {
        self.uri
    }

    pub fn method(&self) -> &Method {
        self.method
    }

    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        self.headers
    }

    pub fn body(&self) -> &SignableBody<'_> {
        &self.body
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum SignableBody<'a> {
    /// A body composed of a slice of bytes
    Bytes(&'a [u8]),
    /// An unsigned payload
    ///
    /// UnsignedPayload is used for streaming requests where the contents of the body cannot be
    /// known prior to signing
    UnsignedPayload,

    /// A precomputed body checksum. The checksum should be a SHA256 checksum of the body,
    /// lowercase hex encoded. Eg:
    /// `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
    Precomputed(String),
}

pub struct SigningMemo {
    headers: Option<HeaderMap<HeaderValue>>,
    params: Option<Vec<(&'static str, Cow<'static, str>)>>,
}

impl SigningMemo {
    fn new(
        headers: Option<HeaderMap<HeaderValue>>,
        params: Option<Vec<(&'static str, Cow<'static, str>)>>,
    ) -> Self {
        Self { headers, params }
    }

    pub fn headers(&self) -> Option<&HeaderMap<HeaderValue>> {
        self.headers.as_ref()
    }
    pub fn take_headers(&mut self) -> Option<HeaderMap<HeaderValue>> {
        self.headers.take()
    }

    pub fn params(&self) -> Option<&Vec<(&'static str, Cow<'static, str>)>> {
        self.params.as_ref()
    }
    pub fn take_params(&mut self) -> Option<Vec<(&'static str, Cow<'static, str>)>> {
        self.params.take()
    }

    // TODO(PresignedReqPrototype): unit test
    pub fn apply_to_request<B>(mut self, request: &mut http::Request<B>) {
        if let Some(new_headers) = self.take_headers() {
            for (name, value) in new_headers.into_iter() {
                request.headers_mut().insert(name.unwrap(), value);
            }
        }
        if let Some(params) = self.take_params() {
            let mut query = QueryWriter::new(request.uri());
            for (name, value) in params {
                query.insert(name, &value);
            }
            *request.uri_mut() = query.build_uri();
        }
    }
}

/// Produces a signature for the given `request` and returns a memo
/// that can be used to apply that signature to an HTTP request.
pub fn sign<'a>(
    request: SignableRequest<'a>,
    params: &'a SigningParams<'a>,
) -> Result<SigningOutput<SigningMemo>, Error> {
    match params.settings.signature_location {
        SignatureLocation::Headers => {
            let (signing_headers, signature) =
                calculate_signing_headers(&request, params)?.into_parts();
            Ok(SigningOutput::new(
                SigningMemo::new(Some(signing_headers), None),
                signature,
            ))
        }
        // TODO(PresignedReqPrototype): Figure out how to write unit tests for this
        SignatureLocation::QueryParams => {
            let (params, signature) = calculate_signing_params(&request, params)?;
            Ok(SigningOutput::new(
                SigningMemo::new(None, Some(params)),
                signature,
            ))
        }
    }
}

type CalculatedParams = Vec<(&'static str, Cow<'static, str>)>;

fn calculate_signing_params<'a>(
    request: &'a SignableRequest<'a>,
    params: &'a SigningParams<'a>,
) -> Result<(CalculatedParams, String), Error> {
    let creq = CanonicalRequest::from(request, params)?;
    tracing::trace!(canonical_request = %creq);

    let encoded_creq = &sha256_hex_string(creq.to_string().as_bytes());
    let sts = StringToSign::new(
        params.date_time,
        &params.region,
        &params.service_name,
        encoded_creq,
    );
    let signing_key = generate_signing_key(
        &params.secret_key,
        params.date_time.date(),
        &params.region,
        &params.service_name,
    );
    let signature = calculate_signature(signing_key, &sts.to_string().as_bytes());

    let values = creq.values.into_query_params().expect("signing with query");
    let mut signing_params = vec![
        ("X-Amz-Algorithm", Cow::Borrowed(values.algorithm)),
        ("X-Amz-Credential", Cow::Owned(values.credential)),
        ("X-Amz-Date", Cow::Owned(values.date_time)),
        ("X-Amz-Expires", Cow::Owned(values.expires)),
        (
            "X-Amz-SignedHeaders",
            Cow::Owned(values.signed_headers.as_str().into()),
        ),
        ("X-Amz-Signature", Cow::Owned(signature.clone())),
    ];
    if let Some(security_token) = params.security_token {
        signing_params.push((X_AMZ_SECURITY_TOKEN, Cow::Owned(security_token.to_string())));
    }
    Ok((signing_params, signature))
}

/// Calculates the signature headers that need to get added to the given `request`.
///
/// `request` MUST NOT contain any of the following headers:
/// - x-amz-date
/// - x-amz-content-sha-256
/// - x-amz-security-token
fn calculate_signing_headers<'a>(
    request: &'a SignableRequest<'a>,
    params: &'a SigningParams<'a>,
) -> Result<SigningOutput<HeaderMap<HeaderValue>>, Error> {
    // Step 1: https://docs.aws.amazon.com/en_pv/general/latest/gr/sigv4-create-canonical-request.html.
    let creq = CanonicalRequest::from(request, params)?;
    tracing::trace!(canonical_request = %creq);

    // Step 2: https://docs.aws.amazon.com/en_pv/general/latest/gr/sigv4-create-string-to-sign.html.
    let encoded_creq = &sha256_hex_string(creq.to_string().as_bytes());
    let sts = StringToSign::new(
        params.date_time,
        params.region,
        params.service_name,
        encoded_creq,
    );

    // Step 3: https://docs.aws.amazon.com/en_pv/general/latest/gr/sigv4-calculate-signature.html
    let signing_key = generate_signing_key(
        params.secret_key,
        params.date_time.date(),
        params.region,
        params.service_name,
    );
    let signature = calculate_signature(signing_key, &sts.to_string().as_bytes());

    // Step 4: https://docs.aws.amazon.com/en_pv/general/latest/gr/sigv4-add-signature-to-request.html
    let values = creq.values.as_headers().expect("signing with headers");
    let mut headers = HeaderMap::new();
    add_header(&mut headers, X_AMZ_DATE, &values.date_time);
    headers.insert(
        "authorization",
        build_authorization_header(params.access_key, &creq, sts, &signature),
    );
    if params.settings.payload_checksum_kind == PayloadChecksumKind::XAmzSha256 {
        add_header(&mut headers, X_AMZ_CONTENT_SHA_256, &values.content_sha256);
    }
    if let Some(security_token) = values.security_token {
        add_header(&mut headers, X_AMZ_SECURITY_TOKEN, security_token);
    }
    Ok(SigningOutput::new(headers, signature))
}

fn add_header(map: &mut HeaderMap<HeaderValue>, key: &'static str, value: &str) {
    map.insert(key, HeaderValue::try_from(value).expect(key));
}

// add signature to authorization header
// Authorization: algorithm Credential=access key ID/credential scope, SignedHeaders=SignedHeaders, Signature=signature
fn build_authorization_header(
    access_key: &str,
    creq: &CanonicalRequest,
    sts: StringToSign,
    signature: &str,
) -> HeaderValue {
    let mut value = HeaderValue::try_from(format!(
        "{} Credential={}/{}, SignedHeaders={}, Signature={}",
        HMAC_256,
        access_key,
        sts.scope.to_string(),
        creq.values.signed_headers().as_str(),
        signature
    ))
    .unwrap();
    value.set_sensitive(true);
    value
}

#[cfg(test)]
mod tests {
    use super::sign;
    use crate::date_fmt::parse_date_time;
    use crate::http_request::sign::SignableRequest;
    use crate::http_request::test::{make_headers_comparable, test_request, test_signed_request};
    use crate::http_request::{SigningParams, SigningSettings};
    use pretty_assertions::assert_eq;

    macro_rules! assert_req_eq {
        ($a:tt, $b:tt) => {
            make_headers_comparable(&mut $a);
            make_headers_comparable(&mut $b);
            assert_eq!(format!("{:?}", $a), format!("{:?}", $b))
        };
    }

    #[test]
    fn test_sign_vanilla() {
        let settings = SigningSettings::default();
        let params = SigningParams {
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            security_token: None,
            region: "us-east-1",
            service_name: "service",
            date_time: parse_date_time("20150830T123600Z").unwrap(),
            settings,
        };

        let original = test_request("get-vanilla-query-order-key-case");
        let signable = SignableRequest::from_http(&original);
        let out = sign(signable, &params).unwrap();
        assert_eq!(
            "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500",
            out.signature
        );

        let mut signed = original;
        out.output.apply_to_request(&mut signed);

        let mut expected = test_signed_request("get-vanilla-query-order-key-case");
        assert_req_eq!(expected, signed);
    }
}
