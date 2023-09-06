/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use super::{
    Action, BodyData, ConnectionId, Direction, Error, Event, NetworkTraffic, Request, Response,
    Version,
};
use aws_smithy_http::body::SdkBody;
use aws_smithy_runtime_api::client::connectors::{
    HttpConnector, HttpConnectorFuture, SharedHttpConnector,
};
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
use http_body::Body;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::{fs, io};
use tokio::task::JoinHandle;

/// Recording Connection Wrapper
///
/// RecordingConnector wraps an inner connection and records all traffic, enabling traffic replay.
#[derive(Clone, Debug)]
pub struct RecordingConnector {
    pub(crate) data: Arc<Mutex<Vec<Event>>>,
    pub(crate) num_events: Arc<AtomicUsize>,
    pub(crate) inner: SharedHttpConnector,
}

#[cfg(all(feature = "tls-rustls"))]
impl RecordingConnector {
    /// Construct a recording connection wrapping a default HTTPS implementation
    pub fn https() -> Self {
        use crate::client::connectors::hyper_connector::HyperConnector;
        Self {
            data: Default::default(),
            num_events: Arc::new(AtomicUsize::new(0)),
            inner: SharedHttpConnector::new(HyperConnector::builder().build_https()),
        }
    }
}

impl RecordingConnector {
    /// Create a new recording connection from a connection
    pub fn new(underlying_connector: SharedHttpConnector) -> Self {
        Self {
            data: Default::default(),
            num_events: Arc::new(AtomicUsize::new(0)),
            inner: underlying_connector,
        }
    }

    /// Return the traffic recorded by this connection
    pub fn events(&self) -> MutexGuard<'_, Vec<Event>> {
        self.data.lock().unwrap()
    }

    /// NetworkTraffic struct suitable for serialization
    pub fn network_traffic(&self) -> NetworkTraffic {
        NetworkTraffic {
            events: self.events().clone(),
            docs: Some("todo docs".into()),
            version: Version::V0,
        }
    }

    /// Dump the network traffic to a file
    pub fn dump_to_file(&self, path: impl AsRef<Path>) -> Result<(), io::Error> {
        fs::write(
            path,
            serde_json::to_string(&self.network_traffic()).unwrap(),
        )
    }

    fn next_id(&self) -> ConnectionId {
        ConnectionId(self.num_events.fetch_add(1, Ordering::Relaxed))
    }
}

fn record_body(
    body: &mut SdkBody,
    event_id: ConnectionId,
    direction: Direction,
    event_bus: Arc<Mutex<Vec<Event>>>,
) -> JoinHandle<()> {
    let (sender, output_body) = hyper::Body::channel();
    let real_body = std::mem::replace(body, SdkBody::from(output_body));
    tokio::spawn(async move {
        let mut real_body = real_body;
        let mut sender = sender;
        loop {
            let data = real_body.data().await;
            match data {
                Some(Ok(data)) => {
                    event_bus.lock().unwrap().push(Event {
                        connection_id: event_id,
                        action: Action::Data {
                            data: BodyData::from(data.clone()),
                            direction,
                        },
                    });
                    // This happens if the real connection is closed during recording.
                    // Need to think more carefully if this is the correct thing to log in this
                    // case.
                    if sender.send_data(data).await.is_err() {
                        event_bus.lock().unwrap().push(Event {
                            connection_id: event_id,
                            action: Action::Eof {
                                direction: direction.opposite(),
                                ok: false,
                            },
                        })
                    };
                }
                None => {
                    event_bus.lock().unwrap().push(Event {
                        connection_id: event_id,
                        action: Action::Eof {
                            ok: true,
                            direction,
                        },
                    });
                    drop(sender);
                    break;
                }
                Some(Err(_err)) => {
                    event_bus.lock().unwrap().push(Event {
                        connection_id: event_id,
                        action: Action::Eof {
                            ok: false,
                            direction,
                        },
                    });
                    sender.abort();
                    break;
                }
            }
        }
    })
}

impl HttpConnector for RecordingConnector {
    fn call(&self, mut request: HttpRequest) -> HttpConnectorFuture {
        let event_id = self.next_id();
        // A request has three phases:
        // 1. A "Request" phase. This is initial HTTP request, headers, & URI
        // 2. A body phase. This may contain multiple data segments.
        // 3. A finalization phase. An EOF of some sort is sent on the body to indicate that
        // the channel should be closed.

        // Phase 1: the initial http request
        self.data.lock().unwrap().push(Event {
            connection_id: event_id,
            action: Action::Request {
                request: Request::from(&request),
            },
        });

        // Phase 2: Swap out the real request body for one that will log all traffic that passes
        // through it
        // This will also handle phase three when the request body runs out of data.
        record_body(
            request.body_mut(),
            event_id,
            Direction::Request,
            self.data.clone(),
        );
        let events = self.data.clone();
        // create a channel we'll use to stream the data while reading it
        let resp_fut = self.inner.call(request);
        let fut = async move {
            let resp = resp_fut.await;
            match resp {
                Ok(mut resp) => {
                    // push the initial response event
                    events.lock().unwrap().push(Event {
                        connection_id: event_id,
                        action: Action::Response {
                            response: Ok(Response::from(&resp)),
                        },
                    });

                    // instrument the body and record traffic
                    record_body(resp.body_mut(), event_id, Direction::Response, events);
                    Ok(resp)
                }
                Err(e) => {
                    events.lock().unwrap().push(Event {
                        connection_id: event_id,
                        action: Action::Response {
                            response: Err(Error(format!("{}", &e))),
                        },
                    });
                    Err(e)
                }
            }
        };
        HttpConnectorFuture::new(fut)
    }
}
