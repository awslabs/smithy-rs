/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::expiring_cache::ExpiringCache;
use aws_smithy_async::future::timeout::Timeout;
use aws_smithy_async::rt::sleep::{default_async_sleep, AsyncSleep, SharedAsyncSleep};
use aws_smithy_async::time::{SharedTimeSource, TimeSource};
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::identity::{
    Identity, IdentityCachePartition, IdentityFuture, IdentityResolver, ResolveCachedIdentity,
    SharedIdentityCache, SharedIdentityResolver,
};
use aws_smithy_runtime_api::shared::IntoShared;
use aws_smithy_types::config_bag::ConfigBag;
use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;
use std::time::Duration;
use tracing::Instrument;

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_EXPIRATION: Duration = Duration::from_secs(15 * 60);
const DEFAULT_BUFFER_TIME: Duration = Duration::from_secs(10);
const DEFAULT_BUFFER_TIME_JITTER_FRACTION: fn() -> f64 = fastrand::f64;

/// Builder for lazy identity caching.
#[derive(Default, Debug)]
pub struct LazyCacheBuilder {
    time_source: Option<SharedTimeSource>,
    sleep_impl: Option<SharedAsyncSleep>,
    load_timeout: Option<Duration>,
    buffer_time: Option<Duration>,
    buffer_time_jitter_fraction: Option<fn() -> f64>,
    default_expiration: Option<Duration>,
}

impl LazyCacheBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the time source for this cache.
    pub fn time_source(mut self, time_source: impl TimeSource + 'static) -> Self {
        self.set_time_source(time_source.into_shared());
        self
    }
    /// Set the time source for this cache.
    pub fn set_time_source(&mut self, time_source: SharedTimeSource) -> &mut Self {
        self.time_source = Some(time_source.into_shared());
        self
    }

    /// Set the async sleep implementation for this cache.
    pub fn sleep_impl(mut self, sleep_impl: impl AsyncSleep + 'static) -> Self {
        self.set_sleep_impl(sleep_impl.into_shared());
        self
    }
    /// Set the async sleep implementation for this cache.
    pub fn set_sleep_impl(&mut self, sleep_impl: SharedAsyncSleep) -> &mut Self {
        self.sleep_impl = Some(sleep_impl);
        self
    }

    /// Timeout for identity resolution.
    ///
    /// Defaults to 5 seconds.
    pub fn load_timeout(mut self, timeout: Duration) -> Self {
        self.set_load_timeout(Some(timeout));
        self
    }

    /// Timeout for identity resolution.
    ///
    /// Defaults to 5 seconds.
    pub fn set_load_timeout(&mut self, timeout: Option<Duration>) -> &mut Self {
        self.load_timeout = timeout;
        self
    }

    /// Amount of time before the actual identity expiration time where the identity is considered expired.
    ///
    /// For example, if the identity are expiring in 15 minutes, and the buffer time is 10 seconds,
    /// then any requests made after 14 minutes and 50 seconds will load a new identity.
    ///
    /// Defaults to 10 seconds.
    pub fn buffer_time(mut self, buffer_time: Duration) -> Self {
        self.set_buffer_time(Some(buffer_time));
        self
    }

    /// Amount of time before the actual identity expiration time where the identity is considered expired.
    ///
    /// For example, if the identity are expiring in 15 minutes, and the buffer time is 10 seconds,
    /// then any requests made after 14 minutes and 50 seconds will load a new identity.
    ///
    /// Defaults to 10 seconds.
    pub fn set_buffer_time(&mut self, buffer_time: Option<Duration>) -> &mut Self {
        self.buffer_time = buffer_time;
        self
    }

    /// A random percentage by which buffer time is jittered for randomization.
    ///
    /// For example, if the identity is expiring in 15 minutes, the buffer time is 10 seconds,
    /// and buffer time jitter fraction is 0.2, then buffer time is adjusted to 8 seconds.
    /// Therefore, any requests made after 14 minutes and 52 seconds will load a new identity.
    ///
    /// Defaults to a randomly generated value between 0.0 and 1.0. This setter is for testing only.
    #[cfg(feature = "test-util")]
    pub fn buffer_time_jitter_fraction(mut self, buffer_time_jitter_fraction: fn() -> f64) -> Self {
        self.set_buffer_time_jitter_fraction(Some(buffer_time_jitter_fraction));
        self
    }

    /// A random percentage by which buffer time is jittered for randomization.
    ///
    /// For example, if the identity is expiring in 15 minutes, the buffer time is 10 seconds,
    /// and buffer time jitter fraction is 0.2, then buffer time is adjusted to 8 seconds.
    /// Therefore, any requests made after 14 minutes and 52 seconds will load a new identity.
    ///
    /// Defaults to a randomly generated value between 0.0 and 1.0. This setter is for testing only.
    #[cfg(feature = "test-util")]
    pub fn set_buffer_time_jitter_fraction(
        &mut self,
        buffer_time_jitter_fraction: Option<fn() -> f64>,
    ) -> &mut Self {
        self.buffer_time_jitter_fraction = buffer_time_jitter_fraction;
        self
    }

    /// Default expiration time to set on an identity if it doesn't have an expiration time.
    ///
    /// This is only used if the resolved identity doesn't have an expiration time set.
    /// This must be at least 15 minutes.
    ///
    /// Defaults to 15 minutes.
    pub fn default_expiration(mut self, duration: Duration) -> Self {
        self.set_default_expiration(Some(duration));
        self
    }

    /// Default expiration time to set on an identity if it doesn't have an expiration time.
    ///
    /// This is only used if the resolved identity doesn't have an expiration time set.
    /// This must be at least 15 minutes.
    ///
    /// Defaults to 15 minutes.
    pub fn set_default_expiration(&mut self, duration: Option<Duration>) -> &mut Self {
        self.default_expiration = duration;
        self
    }

    /// Builds a [`SharedIdentityCache`] from this builder.
    ///
    /// # Panics
    ///
    /// This builder will panic if required fields are not given, or if given values are not valid.
    pub fn build(self) -> SharedIdentityCache {
        let default_expiration = self.default_expiration.unwrap_or(DEFAULT_EXPIRATION);
        assert!(
            default_expiration >= DEFAULT_EXPIRATION,
            "default_expiration must be at least 15 minutes"
        );
        LazyCache::new(
            self.time_source.unwrap_or_default(),
            self.sleep_impl.unwrap_or_else(|| {
                default_async_sleep().expect("no default sleep implementation available")
            }),
            self.load_timeout.unwrap_or(DEFAULT_LOAD_TIMEOUT),
            self.buffer_time.unwrap_or(DEFAULT_BUFFER_TIME),
            self.buffer_time_jitter_fraction
                .unwrap_or(DEFAULT_BUFFER_TIME_JITTER_FRACTION),
            default_expiration,
        )
        .into_shared()
    }
}

#[derive(Debug)]
struct CachePartitions {
    partitions: RwLock<HashMap<IdentityCachePartition, ExpiringCache<Identity, BoxError>>>,
    buffer_time: Duration,
}

impl CachePartitions {
    fn new(buffer_time: Duration) -> Self {
        Self {
            partitions: RwLock::new(HashMap::new()),
            buffer_time,
        }
    }

    fn partition(&self, key: IdentityCachePartition) -> ExpiringCache<Identity, BoxError> {
        let mut partition = self.partitions.read().unwrap().get(&key).cloned();
        if partition.is_none() {
            let mut partitions = self.partitions.write().unwrap();
            // Another thread could have inserted the partition before we acquired the lock,
            // so double check before inserting it.
            partitions
                .entry(key)
                .or_insert_with(|| ExpiringCache::new(self.buffer_time));
            drop(partitions);

            partition = self.partitions.read().unwrap().get(&key).cloned();
        }
        partition.expect("inserted above if not present")
    }
}

#[derive(Debug)]
struct LazyCache {
    time: SharedTimeSource,
    sleeper: SharedAsyncSleep,
    partitions: CachePartitions,
    load_timeout: Duration,
    buffer_time: Duration,
    buffer_time_jitter_fraction: fn() -> f64,
    default_expiration: Duration,
}

impl LazyCache {
    fn new(
        time: SharedTimeSource,
        sleeper: SharedAsyncSleep,
        load_timeout: Duration,
        buffer_time: Duration,
        buffer_time_jitter_fraction: fn() -> f64,
        default_expiration: Duration,
    ) -> Self {
        Self {
            time,
            sleeper,
            partitions: CachePartitions::new(buffer_time),
            load_timeout,
            buffer_time,
            buffer_time_jitter_fraction,
            default_expiration,
        }
    }
}

impl ResolveCachedIdentity for LazyCache {
    fn resolve_cached_identity<'a>(
        &'a self,
        resolver: SharedIdentityResolver,
        config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a>
    where
        Self: 'a,
    {
        let time = self.time.clone();
        let now = self.time.now();
        let timeout_future = self.sleeper.sleep(self.load_timeout);
        let load_timeout = self.load_timeout;
        let cache = self.partitions.partition(resolver.cache_partition());
        let default_expiration = self.default_expiration;

        IdentityFuture::new(async move {
            // Attempt to get cached identity, or clear the cache if they're expired
            if let Some(identity) = cache.yield_or_clear_if_expired(now).await {
                tracing::debug!("loaded identity from cache");
                Ok(identity)
            } else {
                // If we didn't get identity from the cache, then we need to try and load.
                // There may be other threads also loading simultaneously, but this is OK
                // since the futures are not eagerly executed, and the cache will only run one
                // of them.
                let start_time = time.now();
                let result = cache
                    .get_or_load(|| {
                        let span = tracing::info_span!("lazy_load_identity");
                        async move {
                            let fut =
                                Timeout::new(resolver.resolve_identity(config_bag), timeout_future);
                            let identity = match fut.await {
                                Ok(result) => result?,
                                Err(_err) => match resolver.fallback_on_interrupt() {
                                    Some(identity) => identity,
                                    None => {
                                        return Err(BoxError::from(TimedOutError(load_timeout)))
                                    }
                                },
                            };
                            // If the identity don't have an expiration time, then create a default one
                            let expiration =
                                identity.expiration().unwrap_or(now + default_expiration);

                            let jitter = self
                                .buffer_time
                                .mul_f64((self.buffer_time_jitter_fraction)());

                            // Logging for cache miss should be emitted here as opposed to after the call to
                            // `cache.get_or_load` above. In the case of multiple threads concurrently executing
                            // `cache.get_or_load`, logging inside `cache.get_or_load` ensures that it is emitted
                            // only once for the first thread that succeeds in populating a cache value.
                            tracing::info!(
                                "identity cache miss occurred; added new AWS identity (took {:?})",
                                time.now().duration_since(start_time)
                            );

                            Ok((identity, expiration + jitter))
                        }
                        // Only instrument the the actual load future so that no span
                        // is opened if the cache decides not to execute it.
                        .instrument(span)
                    })
                    .await;
                tracing::debug!("loaded identity");
                result
            }
        })
    }
}

#[derive(Debug)]
struct TimedOutError(Duration);

impl std::error::Error for TimedOutError {}

impl fmt::Display for TimedOutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "identity resolver timed out after {:?}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_async::rt::sleep::{SharedAsyncSleep, TokioSleep};
    use aws_smithy_async::test_util::{instant_time_and_sleep, ManualTimeSource};
    use aws_smithy_async::time::{SharedTimeSource, TimeSource};
    use aws_smithy_runtime_api::client::identity::http::Token;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tracing::info;

    const BUFFER_TIME_NO_JITTER: fn() -> f64 = || 0_f64;

    struct ResolverFn<F>(F);
    impl<F> fmt::Debug for ResolverFn<F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("ResolverFn")
        }
    }
    impl<F> IdentityResolver for ResolverFn<F>
    where
        F: Fn() -> IdentityFuture<'static> + Send + Sync,
    {
        fn resolve_identity<'a>(&'a self, _config_bag: &'a ConfigBag) -> IdentityFuture<'a> {
            (self.0)()
        }
    }

    fn resolver_fn<F>(f: F) -> SharedIdentityResolver
    where
        F: Fn() -> IdentityFuture<'static> + Send + Sync + 'static,
    {
        SharedIdentityResolver::new(ResolverFn(f))
    }

    fn test_cache(
        time: impl TimeSource + 'static,
        buffer_time_jitter_fraction: fn() -> f64,
        load_list: Vec<Result<Identity, BoxError>>,
    ) -> (LazyCache, SharedIdentityResolver) {
        #[derive(Debug)]
        struct Resolver(Mutex<Vec<Result<Identity, BoxError>>>);
        impl IdentityResolver for Resolver {
            fn resolve_identity<'a>(&'a self, _config_bag: &'a ConfigBag) -> IdentityFuture<'a> {
                let mut list = self.0.lock().unwrap();
                if list.len() > 0 {
                    let next = list.remove(0);
                    info!("refreshing the identity to {:?}", next);
                    IdentityFuture::ready(next)
                } else {
                    drop(list);
                    panic!("no more identities")
                }
            }
        }

        let identity_resolver = SharedIdentityResolver::new(Resolver(Mutex::new(load_list)));
        let cache = LazyCache::new(
            SharedTimeSource::new(time),
            SharedAsyncSleep::new(TokioSleep::new()),
            DEFAULT_LOAD_TIMEOUT,
            DEFAULT_BUFFER_TIME,
            buffer_time_jitter_fraction,
            DEFAULT_EXPIRATION,
        );
        (cache, identity_resolver)
    }

    fn epoch_secs(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn test_identity(expired_secs: u64) -> Identity {
        let expiration = Some(epoch_secs(expired_secs));
        Identity::new(Token::new("test", expiration), expiration)
    }

    async fn expect_identity(
        expired_secs: u64,
        cache: &LazyCache,
        resolver: SharedIdentityResolver,
    ) {
        let config_bag = ConfigBag::base();
        let identity = cache
            .resolve_cached_identity(resolver, &config_bag)
            .await
            .expect("expected identity");
        assert_eq!(Some(epoch_secs(expired_secs)), identity.expiration());
    }

    #[tokio::test]
    async fn initial_populate_test_identity() {
        let time = ManualTimeSource::new(UNIX_EPOCH);
        let config_bag = ConfigBag::base();
        let resolver = SharedIdentityResolver::new(resolver_fn(|| {
            info!("refreshing the test_identity");
            IdentityFuture::ready(Ok(test_identity(1000)))
        }));
        let cache = LazyCache::new(
            SharedTimeSource::new(time),
            SharedAsyncSleep::new(TokioSleep::new()),
            DEFAULT_LOAD_TIMEOUT,
            DEFAULT_BUFFER_TIME,
            BUFFER_TIME_NO_JITTER,
            DEFAULT_EXPIRATION,
        );
        assert_eq!(
            epoch_secs(1000),
            cache
                .resolve_cached_identity(resolver, &config_bag)
                .await
                .unwrap()
                .expiration()
                .unwrap()
        );
    }

    #[tokio::test]
    async fn reload_expired_test_identity() {
        let time = ManualTimeSource::new(epoch_secs(100));
        let (cache, resolver) = test_cache(
            time.clone(),
            BUFFER_TIME_NO_JITTER,
            vec![
                Ok(test_identity(1000)),
                Ok(test_identity(2000)),
                Ok(test_identity(3000)),
            ],
        );

        expect_identity(1000, &cache, resolver.clone()).await;
        expect_identity(1000, &cache, resolver.clone()).await;
        time.set_time(epoch_secs(1500));
        expect_identity(2000, &cache, resolver.clone()).await;
        expect_identity(2000, &cache, resolver.clone()).await;
        time.set_time(epoch_secs(2500));
        expect_identity(3000, &cache, resolver.clone()).await;
        expect_identity(3000, &cache, resolver.clone()).await;
    }

    #[tokio::test]
    async fn load_failed_error() {
        let config_bag = ConfigBag::base();
        let time = ManualTimeSource::new(epoch_secs(100));
        let (cache, resolver) = test_cache(
            time.clone(),
            BUFFER_TIME_NO_JITTER,
            vec![Ok(test_identity(1000)), Err("failed".into())],
        );

        expect_identity(1000, &cache, resolver.clone()).await;
        time.set_time(epoch_secs(1500));
        assert!(cache
            .resolve_cached_identity(resolver.clone(), &config_bag)
            .await
            .is_err());
    }

    #[test]
    fn load_contention() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .worker_threads(16)
            .build()
            .unwrap();

        let time = ManualTimeSource::new(epoch_secs(0));
        let (cache, resolver) = test_cache(
            time.clone(),
            BUFFER_TIME_NO_JITTER,
            vec![
                Ok(test_identity(500)),
                Ok(test_identity(1500)),
                Ok(test_identity(2500)),
                Ok(test_identity(3500)),
                Ok(test_identity(4500)),
            ],
        );
        let cache: SharedIdentityCache = cache.into_shared();

        // test_identity are available up until 4500 seconds after the unix epoch
        // 4*50 = 200 tasks are launched => we can advance time 4500/20 => 225 seconds per advance
        for _ in 0..4 {
            let mut tasks = Vec::new();
            for _ in 0..50 {
                let resolver = resolver.clone();
                let cache = cache.clone();
                let time = time.clone();
                tasks.push(rt.spawn(async move {
                    let now = time.advance(Duration::from_secs(22));

                    let config_bag = ConfigBag::base();
                    let identity = cache
                        .resolve_cached_identity(resolver, &config_bag)
                        .await
                        .unwrap();
                    assert!(
                        identity.expiration().unwrap() >= now,
                        "{:?} >= {:?}",
                        identity.expiration(),
                        now
                    );
                }));
            }
            for task in tasks {
                rt.block_on(task).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn load_timeout() {
        let config_bag = ConfigBag::base();
        let (time, sleep) = instant_time_and_sleep(epoch_secs(100));
        let resolver = SharedIdentityResolver::new(resolver_fn(|| {
            IdentityFuture::new(async {
                aws_smithy_async::future::never::Never::new().await;
                Ok(test_identity(1000))
            })
        }));
        let cache = LazyCache::new(
            SharedTimeSource::new(time.clone()),
            SharedAsyncSleep::new(sleep),
            Duration::from_secs(5),
            DEFAULT_BUFFER_TIME,
            BUFFER_TIME_NO_JITTER,
            DEFAULT_EXPIRATION,
        );

        let err: BoxError = cache
            .resolve_cached_identity(resolver, &config_bag)
            .await
            .expect_err("it should return an error");
        let downcasted = err.downcast_ref::<TimedOutError>();
        assert!(
            downcasted.is_some(),
            "expected a BoxError of TimedOutError, but was {err:?}"
        );
        assert_eq!(time.now(), epoch_secs(105));
    }

    #[tokio::test]
    async fn buffer_time_jitter() {
        let time = ManualTimeSource::new(epoch_secs(100));
        let buffer_time_jitter_fraction = || 0.5_f64;
        let (cache, resolver) = test_cache(
            time.clone(),
            buffer_time_jitter_fraction,
            vec![Ok(test_identity(1000)), Ok(test_identity(2000))],
        );

        expect_identity(1000, &cache, resolver.clone()).await;
        let buffer_time_with_jitter =
            (DEFAULT_BUFFER_TIME.as_secs_f64() * buffer_time_jitter_fraction()) as u64;
        assert_eq!(buffer_time_with_jitter, 5);
        // Advance time to the point where the first test_identity are about to expire (but haven't).
        let almost_expired_secs = 1000 - buffer_time_with_jitter - 1;
        time.set_time(epoch_secs(almost_expired_secs));
        // We should still use the first test_identity.
        expect_identity(1000, &cache, resolver.clone()).await;
        // Now let the first test_identity expire.
        let expired_secs = almost_expired_secs + 1;
        time.set_time(epoch_secs(expired_secs));
        // Now that the first test_identity have been expired, the second test_identity will be retrieved.
        expect_identity(2000, &cache, resolver.clone()).await;
    }

    #[tokio::test]
    async fn cache_partitioning() {
        let time = ManualTimeSource::new(epoch_secs(0));
        let (cache, _) = test_cache(time.clone(), BUFFER_TIME_NO_JITTER, Vec::new());

        let far_future = SystemTime::now() + Duration::from_secs(10_000);

        // Resolver A and B both return an identical identity type with different tokens with an expiration
        // time that should NOT be hit within this test. They each have their own partition key.
        let resolver_a_calls = Arc::new(AtomicUsize::new(0));
        let resolver_b_calls = Arc::new(AtomicUsize::new(0));
        let resolver_a = resolver_fn({
            let calls = resolver_a_calls.clone();
            move || {
                calls.fetch_add(1, Ordering::Relaxed);
                IdentityFuture::ready(Ok(Identity::new(
                    Token::new("A", Some(far_future)),
                    Some(far_future),
                )))
            }
        });
        let resolver_b = resolver_fn({
            let calls = resolver_b_calls.clone();
            move || {
                calls.fetch_add(1, Ordering::Relaxed);
                IdentityFuture::ready(Ok(Identity::new(
                    Token::new("B", Some(far_future)),
                    Some(far_future),
                )))
            }
        });
        assert_ne!(
            resolver_a.cache_partition(),
            resolver_b.cache_partition(),
            "pre-condition: they should have different partition keys"
        );

        let config_bag = ConfigBag::base();

        // Loading the identity twice with resolver A should result in a single call
        // to the underlying identity resolver since the result gets cached.
        let identity = cache
            .resolve_cached_identity(resolver_a.clone(), &config_bag)
            .await
            .unwrap();
        assert_eq!("A", identity.data::<Token>().unwrap().token());
        let identity = cache
            .resolve_cached_identity(resolver_a.clone(), &config_bag)
            .await
            .unwrap();
        assert_eq!("A", identity.data::<Token>().unwrap().token());
        assert_eq!(1, resolver_a_calls.load(Ordering::Relaxed));

        // Now, loading an identity from B will use a separate cache partition
        // and return a different result.
        let identity = cache
            .resolve_cached_identity(resolver_b.clone(), &config_bag)
            .await
            .unwrap();
        assert_eq!("B", identity.data::<Token>().unwrap().token());
        let identity = cache
            .resolve_cached_identity(resolver_b.clone(), &config_bag)
            .await
            .unwrap();
        assert_eq!("B", identity.data::<Token>().unwrap().token());
        assert_eq!(1, resolver_a_calls.load(Ordering::Relaxed));
        assert_eq!(1, resolver_b_calls.load(Ordering::Relaxed));

        // Finally, loading with resolver A again should return the original cached A value
        let identity = cache
            .resolve_cached_identity(resolver_a.clone(), &config_bag)
            .await
            .unwrap();
        assert_eq!("A", identity.data::<Token>().unwrap().token());
        assert_eq!(1, resolver_a_calls.load(Ordering::Relaxed));
        assert_eq!(1, resolver_b_calls.load(Ordering::Relaxed));
    }
}
