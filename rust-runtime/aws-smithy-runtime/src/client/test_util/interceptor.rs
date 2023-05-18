/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

// TODO(enableNewSmithyRuntime): Delete this file once test helpers on `CustomizableOperation` have been removed

use aws_smithy_runtime_api::client::interceptors::context::phase::BeforeTransmit;
use aws_smithy_runtime_api::client::interceptors::{BoxError, Interceptor, InterceptorContext};
use aws_smithy_runtime_api::config_bag::ConfigBag;
use std::fmt;

pub struct TestParamsSetterInterceptor<F> {
    f: F,
}

impl<F> fmt::Debug for TestParamsSetterInterceptor<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TestParamsSetterInterceptor")
    }
}

impl<F> TestParamsSetterInterceptor<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F> Interceptor for TestParamsSetterInterceptor<F>
where
    F: Fn(&mut ConfigBag) + Send + Sync + 'static,
{
    fn modify_before_signing(
        &self,
        _context: &mut InterceptorContext<BeforeTransmit>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        (self.f)(cfg);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_runtime_api::type_erasure::TypedBox;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn set_test_request_time() {
        let mut cfg = ConfigBag::base();
        let context = InterceptorContext::<()>::new(TypedBox::new("anything").erase());
        let mut context = context.into_serialization_phase();
        let _ = context.take_input();
        context.set_request(http::Request::builder().body(SdkBody::empty()).unwrap());
        let mut context = context.into_before_transmit_phase();
        let request_time = UNIX_EPOCH + Duration::from_secs(1624036048);
        let interceptor = TestParamsSetterInterceptor::new({
            let request_time = request_time.clone();
            move |cfg: &mut ConfigBag| {
                cfg.put(request_time);
            }
        });
        interceptor
            .modify_before_signing(&mut context, &mut cfg)
            .unwrap();
        assert_eq!(&request_time, cfg.get::<SystemTime>().unwrap());
    }
}
