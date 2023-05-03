/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_runtime_api::client::interceptors::context::OutputOrError;
use aws_smithy_runtime_api::client::interceptors::Interceptors;
use aws_smithy_runtime_api::client::orchestrator::{
    ConfigBagAccessors, HttpResponse, ResponseDeserializer,
};
use aws_smithy_runtime_api::client::runtime_plugin::{BoxError, RuntimePlugin};
use aws_smithy_runtime_api::config_bag::ConfigBag;
use std::sync::Mutex;

#[derive(Default, Debug)]
pub struct CannedResponseDeserializer {
    inner: Mutex<Option<OutputOrError>>,
}

impl CannedResponseDeserializer {
    pub fn new(output: OutputOrError) -> Self {
        Self {
            inner: Mutex::new(Some(output)),
        }
    }

    pub fn take(&self) -> Option<OutputOrError> {
        match self.inner.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        }
    }
}

impl ResponseDeserializer for CannedResponseDeserializer {
    fn deserialize_nonstreaming(&self, _response: &HttpResponse) -> OutputOrError {
        self.take()
            .ok_or("CannedResponseDeserializer's inner value has already been taken.")
            .unwrap()
    }
}

impl RuntimePlugin for CannedResponseDeserializer {
    fn configure(
        &self,
        cfg: &mut ConfigBag,
        _interceptors: &mut Interceptors,
    ) -> Result<(), BoxError> {
        cfg.set_response_deserializer(Self {
            inner: Mutex::new(self.take()),
        });

        Ok(())
    }
}
