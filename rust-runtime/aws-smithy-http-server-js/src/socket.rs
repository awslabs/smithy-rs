/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Socket implementation that can be shared between multiple Python processes.

use aws_smithy_http_server::socket::Socket;
use napi_derive::napi;

/// Socket implementation that can be shared between multiple Python processes.
///
/// Python cannot handle true multi-threaded applications due to the [GIL],
/// often resulting in reduced performance and only one core used by the application.
/// To work around this, Python web applications usually create a socket with
/// SO_REUSEADDR and SO_REUSEPORT enabled that can be shared between multiple
/// Python processes, allowing you to maximize performance and use all available
/// computing capacity of the host.
///
/// [GIL]: https://wiki.python.org/moin/GlobalInterpreterLock
#[napi]
#[derive(Debug)]
pub struct JsSocket(socket2::Socket);

#[napi]
impl JsSocket {
    /// Create a new UNIX `SharedSocket` from an address, port and backlog.
    /// If not specified, the backlog defaults to 1024 connections.
    #[napi(constructor)]
    pub fn new(address: String, port: i32, backlog: Option<i32>) -> napi::Result<Self> {
        Ok(Self(
            Socket::new(address, port, backlog)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?,
        ))
    }

    /// Clone the inner socket allowing it to be shared between multiple
    /// Python processes.
    #[napi]
    pub fn try_clone(&self) -> napi::Result<JsSocket> {
        Ok(JsSocket(
            self.0
                .try_clone()
                .map_err(|e| napi::Error::from_reason(e.to_string()))?,
        ))
    }
}

impl JsSocket {
    pub fn to_raw_socket(&self) -> napi::Result<socket2::Socket> {
        Ok(self
            .0
            .try_clone()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?)
    }
}
