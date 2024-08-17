/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

#[allow(dead_code)]
pub(crate) mod rpc_v2_cbor {
    use aws_smithy_runtime::client::sdk_feature::SmithySdkFeature;
    use aws_smithy_runtime_api::box_error::BoxError;
    use aws_smithy_runtime_api::client::interceptors::context::BeforeSerializationInterceptorContextMut;
    use aws_smithy_runtime_api::client::interceptors::Intercept;
    use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
    use aws_smithy_types::config_bag::ConfigBag;

    #[derive(Debug)]
    pub(crate) struct RpcV2CborFeatureTrackerInterceptor;

    impl RpcV2CborFeatureTrackerInterceptor {
        pub(crate) fn new() -> Self {
            Self
        }
    }

    impl Intercept for RpcV2CborFeatureTrackerInterceptor {
        fn name(&self) -> &'static str {
            "RpcV2CborFeatureTrackerInterceptor"
        }

        fn modify_before_serialization(
            &self,
            _context: &mut BeforeSerializationInterceptorContextMut<'_>,
            _runtime_components: &RuntimeComponents,
            cfg: &mut ConfigBag,
        ) -> Result<(), BoxError> {
            cfg.interceptor_state()
                .store_append::<SmithySdkFeature>(SmithySdkFeature::ProtocolRpcV2Cbor);
            Ok(())
        }
    }
}

#[allow(dead_code)]
pub(crate) mod paginator {
    use aws_smithy_runtime::client::sdk_feature::SmithySdkFeature;
    use aws_smithy_runtime_api::box_error::BoxError;
    use aws_smithy_runtime_api::client::interceptors::context::BeforeSerializationInterceptorContextMut;
    use aws_smithy_runtime_api::client::interceptors::{Intercept, SharedInterceptor};
    use aws_smithy_runtime_api::client::runtime_components::{
        RuntimeComponents, RuntimeComponentsBuilder,
    };
    use aws_smithy_runtime_api::client::runtime_plugin::RuntimePlugin;
    use aws_smithy_types::config_bag::ConfigBag;
    use std::borrow::Cow;

    #[derive(Debug)]
    struct PaginatorFeatureTrackerInterceptor;

    impl PaginatorFeatureTrackerInterceptor {
        pub(crate) fn new() -> Self {
            Self
        }
    }

    impl Intercept for PaginatorFeatureTrackerInterceptor {
        fn name(&self) -> &'static str {
            "PaginatorFeatureTrackerInterceptor"
        }

        fn modify_before_serialization(
            &self,
            _context: &mut BeforeSerializationInterceptorContextMut<'_>,
            _runtime_components: &RuntimeComponents,
            cfg: &mut ConfigBag,
        ) -> Result<(), BoxError> {
            cfg.interceptor_state()
                .store_append::<SmithySdkFeature>(SmithySdkFeature::Paginator);
            Ok(())
        }
    }

    #[derive(Debug)]
    pub(crate) struct PaginatorFeatureTrackerRuntimePlugin {
        runtime_components: RuntimeComponentsBuilder,
    }

    impl PaginatorFeatureTrackerRuntimePlugin {
        pub(crate) fn new() -> Self {
            Self {
                runtime_components: RuntimeComponentsBuilder::new(
                    "PaginatorFeatureTrackerRuntimePlugin",
                )
                .with_interceptor(SharedInterceptor::new(
                    PaginatorFeatureTrackerInterceptor::new(),
                )),
            }
        }
    }

    impl RuntimePlugin for PaginatorFeatureTrackerRuntimePlugin {
        fn runtime_components(
            &self,
            _: &RuntimeComponentsBuilder,
        ) -> Cow<'_, RuntimeComponentsBuilder> {
            Cow::Borrowed(&self.runtime_components)
        }
    }
}

#[allow(dead_code)]
pub(crate) mod waiter {
    use aws_smithy_runtime::client::sdk_feature::SmithySdkFeature;
    use aws_smithy_runtime_api::box_error::BoxError;
    use aws_smithy_runtime_api::client::interceptors::context::BeforeSerializationInterceptorContextMut;
    use aws_smithy_runtime_api::client::interceptors::{Intercept, SharedInterceptor};
    use aws_smithy_runtime_api::client::runtime_components::{
        RuntimeComponents, RuntimeComponentsBuilder,
    };
    use aws_smithy_runtime_api::client::runtime_plugin::RuntimePlugin;
    use aws_smithy_types::config_bag::ConfigBag;
    use std::borrow::Cow;

    #[derive(Debug)]
    struct WaiterFeatureTrackerInterceptor;

    impl WaiterFeatureTrackerInterceptor {
        pub(crate) fn new() -> Self {
            Self
        }
    }

    impl Intercept for WaiterFeatureTrackerInterceptor {
        fn name(&self) -> &'static str {
            "WaiterFeatureTrackerInterceptor"
        }

        fn modify_before_serialization(
            &self,
            _context: &mut BeforeSerializationInterceptorContextMut<'_>,
            _runtime_components: &RuntimeComponents,
            cfg: &mut ConfigBag,
        ) -> Result<(), BoxError> {
            cfg.interceptor_state()
                .store_append::<SmithySdkFeature>(SmithySdkFeature::Waiter);
            Ok(())
        }
    }

    #[derive(Debug)]
    pub(crate) struct WaiterFeatureTrackerRuntimePlugin {
        runtime_components: RuntimeComponentsBuilder,
    }

    impl WaiterFeatureTrackerRuntimePlugin {
        pub(crate) fn new() -> Self {
            Self {
                runtime_components: RuntimeComponentsBuilder::new(
                    "WaiterFeatureTrackerRuntimePlugin",
                )
                .with_interceptor(SharedInterceptor::new(
                    WaiterFeatureTrackerInterceptor::new(),
                )),
            }
        }
    }

    impl RuntimePlugin for WaiterFeatureTrackerRuntimePlugin {
        fn runtime_components(
            &self,
            _: &RuntimeComponentsBuilder,
        ) -> Cow<'_, RuntimeComponentsBuilder> {
            Cow::Borrowed(&self.runtime_components)
        }
    }
}
