/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_runtime::{BoxError, HttpResponse, ResponseDeserializer};
use aws_smithy_runtime_api::config_bag::ConfigBag;
use aws_smithy_runtime_api::interceptors::context::OutputOrError;
use aws_smithy_runtime_api::runtime_plugin::RuntimePlugin;

#[derive(Debug)]
pub struct GetObjectResponseDeserializer {}

impl GetObjectResponseDeserializer {
    pub fn new() -> Self {
        Self {}
    }
}

impl RuntimePlugin for GetObjectResponseDeserializer {
    fn configure(&self, _cfg: &mut ConfigBag) -> Result<(), BoxError> {
        // TODO(orchestrator) put a deserializer in the bag
        Ok(())
    }
}

impl ResponseDeserializer for GetObjectResponseDeserializer {
    fn deserialize_streaming(&self, _response: &mut HttpResponse) -> Option<OutputOrError> {
        todo!()
    }

    fn deserialize_nonstreaming(&self, _response: &HttpResponse) -> OutputOrError {
        todo!()
    }
}
