/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Metrics are used to gain insight into the operational performance and health of a system in
//! real time.

use crate::attributes::{Attributes, Context, ContextGeneric};
use std::{fmt::Debug, sync::Arc};

/// Provides named instances of [Meter].
pub trait ProvideMeter: Send + Sync + Debug {
    /// Get or create a named [Meter].
    fn get_meter(&self, scope: &'static str, attributes: Option<&Attributes>) -> Arc<dyn Meter>;

    /// Cast to [std::any::Any]
    fn as_any(&self) -> &dyn std::any::Any;
}

/// The entry point to creating instruments. A grouping of related metrics.
pub trait Meter: Send + Sync + Debug {
    /// Create a new Gauge.
    #[allow(clippy::type_complexity)]
    fn create_gauge(
        &self,
        name: String,
        callback: Box<dyn Fn(&dyn AsyncMeasure<Value = f64>) + Send + Sync>,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<dyn AsyncMeasure<Value = f64>>;

    /// Create a new [UpDownCounter].
    fn create_up_down_counter(
        &self,
        name: String,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<dyn UpDownCounter>;

    /// Create a new AsyncUpDownCounter.
    #[allow(clippy::type_complexity)]
    fn create_async_up_down_counter(
        &self,
        name: String,
        callback: Box<dyn Fn(&dyn AsyncMeasure<Value = i64>) + Send + Sync>,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<dyn AsyncMeasure<Value = i64>>;

    /// Create a new [MonotonicCounter].
    fn create_monotonic_counter(
        &self,
        name: String,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<dyn MonotonicCounter>;

    /// Create a new AsyncMonotonicCounter.
    #[allow(clippy::type_complexity)]
    fn create_async_monotonic_counter(
        &self,
        name: String,
        callback: Box<dyn Fn(&dyn AsyncMeasure<Value = u64>) + Send + Sync>,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<dyn AsyncMeasure<Value = u64>>;

    /// Create a new [Histogram].
    fn create_histogram(
        &self,
        name: String,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<dyn Histogram>;
}

/// Collects a set of events with an event count and sum for all events.
pub trait Histogram: Send + Sync + Debug {
    /// Record a value.
    fn record(&self, value: f64, attributes: Option<&Attributes>, context: Option<&dyn Context>);
}

/// A counter that monotonically increases.
pub trait MonotonicCounter: Send + Sync + Debug {
    /// Increment a counter by a fixed amount.
    fn add(&self, value: u64, attributes: Option<&Attributes>, context: Option<&dyn Context>);
}

/// A counter that can increase or decrease.
pub trait UpDownCounter: Send + Sync + Debug {
    /// Increment or decrement a counter by a fixed amount.
    fn add(&self, value: i64, attributes: Option<&Attributes>, context: Option<&dyn Context>);
}

/// A measurement that can be taken asynchronously.
pub trait AsyncMeasure: Send + Sync + Debug {
    /// The type recorded by the measurement.
    type Value;

    /// Record a value
    fn record(
        &self,
        value: Self::Value,
        attributes: Option<&Attributes>,
        context: Option<&dyn Context>,
    );

    /// Stop recording, unregister callback.
    fn stop(&self);
}

pub trait HistogramGeneric<C>: Send + Sync + Debug
where
    C: ContextGeneric,
{
    /// Record a value.
    fn record(&self, value: f64, attributes: Option<&Attributes>, context: Option<&C>);
}

/// A counter that monotonically increases.
pub trait MonotonicCounterGeneric<C>: Send + Sync + Debug
where
    C: ContextGeneric,
{
    /// Increment a counter by a fixed amount.
    fn add(&self, value: u64, attributes: Option<&Attributes>, context: Option<&C>);
}

/// A counter that can increase or decrease.
pub trait UpDownCounterGeneric<C>: Send + Sync + Debug
where
    C: ContextGeneric,
{
    /// Increment or decrement a counter by a fixed amount.
    fn add(&self, value: i64, attributes: Option<&Attributes>, context: Option<&C>);
}

pub trait AsyncMeasureGeneric<'a, C, T>: Send + Sync + Debug
where
    C: ContextGeneric,
{
    /// The type recorded by the measurement.
    // type Value = T;

    /// Record a value
    fn record(&self, value: T, attributes: Option<&Attributes>, context: Option<&C>);

    /// Stop recording, unregister callback.
    fn stop(&self);
}

pub trait MeterGeneric<'a, C, Gauge, UDC, AsyncUDC, MC, AsyncMC, H, FGauge>:
    Send + Sync + Debug
where
    Gauge: AsyncMeasureGeneric<'a, C, f64>,
    UDC: UpDownCounterGeneric<C>,
    AsyncUDC: AsyncMeasureGeneric<'a, C, i64>,
    MC: MonotonicCounterGeneric<C>,
    AsyncMC: AsyncMeasureGeneric<'a, C, u64>,
    H: Histogram,
    C: ContextGeneric,
    FGauge: AsyncMeasureGeneric<'a, C, f64>,
{
    /// Create a new Gauge.
    #[allow(clippy::type_complexity)]
    fn create_gauge(
        &self,
        name: String,
        callback: impl Fn(&FGauge) + Send + Sync + 'static,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<Gauge>;

    /// Create a new [UpDownCounter].
    fn create_up_down_counter(
        &self,
        name: String,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<UDC>;

    /// Create a new AsyncUpDownCounter.
    #[allow(clippy::type_complexity)]
    fn create_async_up_down_counter(
        &self,
        name: String,
        callback: impl Fn(&AsyncUDC) + Send + Sync,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<AsyncUDC>;

    /// Create a new [MonotonicCounter].
    fn create_monotonic_counter(
        &self,
        name: String,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<MC>;

    /// Create a new AsyncMonotonicCounter.
    #[allow(clippy::type_complexity)]
    fn create_async_monotonic_counter(
        &self,
        name: String,
        callback: impl Fn(&AsyncMC) + Send + Sync,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<AsyncMC>;

    /// Create a new [Histogram].
    fn create_histogram(
        &self,
        name: String,
        units: Option<String>,
        description: Option<String>,
    ) -> Arc<H>;
}

// pub trait ProvideMeterGeneric<M, C, Gauge, UDC, AsyncUDC, MC, AsyncMC, H>:
//     Send + Sync + Debug
// where
//     M: MeterGeneric<C, Gauge, UDC, AsyncUDC, MC, AsyncMC, H>,
//     Gauge: AsyncMeasureGeneric<C, Value = f64>,
//     UDC: UpDownCounterGeneric<C>,
//     AsyncUDC: AsyncMeasureGeneric<C, Value = i64>,
//     MC: MonotonicCounterGeneric<C>,
//     AsyncMC: AsyncMeasureGeneric<C, Value = u64>,
//     H: Histogram,
//     C: ContextGeneric,
// {
//     /// Get or create a named [Meter].
//     fn get_meter(&self, scope: &'static str, attributes: Option<&Attributes>) -> Arc<M>;
// }
