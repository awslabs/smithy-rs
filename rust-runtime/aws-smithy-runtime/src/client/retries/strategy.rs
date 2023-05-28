/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

#[cfg(feature = "test-util")]
mod fixed_delay;
mod never;

#[cfg(feature = "test-util")]
pub use fixed_delay::FixedDelayRetryStrategy;
pub use never::NeverRetryStrategy;
