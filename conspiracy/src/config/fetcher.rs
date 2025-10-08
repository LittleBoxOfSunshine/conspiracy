use std::{error::Error, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use conspiracy_theories::config::{ConfigFetcher, RestartRequired};

use crate::{
    config::source::{SourceEtag, TaggedSource, static_source::StaticCacheSource},
    flight::{Evaluator, Flattenable, Flattener, Merge},
};

pub type BoxedTaggedSource<T> = Box<dyn TaggedSource<T> + Send>;
pub type FetcherError = Box<dyn Error + Send + Sync>;
pub type FetcherResult<T> = Result<T, FetcherError>;

struct SourceLayer<T> {
    etag: SourceEtag,
    source: BoxedTaggedSource<T>,
}

impl<T> SourceLayer<T> {
    fn is_fresh(&self) -> bool {
        self.source.is_fresh(self.etag)
    }

    fn fetch(&mut self) -> FetcherResult<T> {
        let (new_etag, config) = self.source.fetch()?;
        self.etag = new_etag;
        Ok(config)
    }
}

// T config type I intermediate config type (e.g. serde_json::Value, toml::Value, etc)
struct InnerFetcherState<C, I, E: Evaluator> {
    current_config: Arc<ArcSwap<C>>,
    sources: InnerSource<I, E>,
}

struct InnerSource<I, E: Evaluator> {
    config_source_layers: Vec<SourceLayer<I>>,
    environment: Option<(SourceLayer<E::Environment>, Flattener<E>)>,
}

impl<C, I: Flattenable + Merge, E: Evaluator<Key = I::Key>> InnerFetcherState<C, I, E> {
    /// Returns before and after if an update was applied.
    fn poll(&mut self) -> Option<(Arc<C>, Arc<C>)>
    where
        I: TryInto<C, Error: Error + Send + Sync + 'static>,
    {
        if let Ok(updated_config) = self.sources.fetch_latest_if_stale()
            && let Some(updated_config) = updated_config
        {
            let updated_config = Arc::new(try_into_config(updated_config).ok()?);
            let current_config = self.current_config.load().clone();
            self.current_config.store(updated_config.clone());
            return Some((current_config, updated_config));
        }

        None
    }
}

impl<I: Flattenable + Merge, E: Evaluator<Key = I::Key>> InnerSource<I, E> {
    fn fetch_latest_if_stale(&mut self) -> FetcherResult<Option<I>> {
        let environment_fresh = self.environment.as_mut().is_none_or(|env| env.0.is_fresh());
        let all_layers_fresh = self
            .config_source_layers
            .iter()
            .all(|layer| layer.is_fresh());

        if !environment_fresh || !all_layers_fresh {
            let latest = self.fetch_latest()?;
            return Ok(Some(latest));
        }

        Ok(None)
    }

    fn fetch_latest(&mut self) -> FetcherResult<I> {
        self.fetch_latest_inner()
            .inspect(|_| tracing::debug!("Applying updated config or environment"))
            .inspect_err(|error| {
                tracing::error!(%error, "Failed to fetch latest config or environment");
            })
    }

    fn fetch_latest_inner(&mut self) -> FetcherResult<I> {
        let mut flattened_config =
            Self::fetch_config_layer(&mut self.environment, &mut self.config_source_layers[0])?;

        for layer in self.config_source_layers.iter_mut().skip(1) {
            let next_config = Self::fetch_config_layer(&mut self.environment, layer)?;
            flattened_config.overriding_merge(next_config);
        }

        Ok(flattened_config)
    }

    fn fetch_config_layer(
        environment: &mut Option<(SourceLayer<E::Environment>, Flattener<E>)>,
        layer: &mut SourceLayer<I>,
    ) -> FetcherResult<I> {
        let mut flattened_config = layer.fetch()?;
        if let Some((environment, flattener)) = environment.as_mut() {
            let env = environment.fetch()?;
            flattener.flatten(&env, &mut flattened_config)?;
        }

        Ok(flattened_config)
    }
}

pub struct ConspiracyConfigFetcher<T> {
    current_config: Arc<ArcSwap<T>>,
}

impl<T> ConspiracyConfigFetcher<T> {
    fn new(initial_config: Arc<ArcSwap<T>>) -> Self {
        Self {
            current_config: initial_config,
        }
    }
}

impl<T> ConfigFetcher<T> for ConspiracyConfigFetcher<T> {
    fn latest_snapshot(&self) -> Arc<T> {
        self.current_config.load().clone()
    }
}

#[derive(Default)]
pub struct ConspiracyConfigFetcherBuilder<T: Flattenable + Merge, Env, Eval> {
    // TODO: Consider avoid Box with generic layer stack
    config_source_layers: Vec<BoxedTaggedSource<T>>,
    environment: Option<(BoxedTaggedSource<Arc<Env>>, Eval)>,
}

impl<
    I: Flattenable + Merge + 'static,
    Env: Send + Sync + 'static,
    Eval: Evaluator<Key = I::Key, Environment = Arc<Env>> + Send + 'static,
> ConspiracyConfigFetcherBuilder<I, Env, Eval>
{
    pub fn new(base_source: BoxedTaggedSource<I>) -> Self {
        Self {
            config_source_layers: vec![base_source],
            environment: None,
        }
    }

    pub fn static_environment(mut self, env: Env, evaluator: Eval) -> Self {
        self.environment = Some((Box::new(StaticCacheSource::new(Arc::new(env))), evaluator));
        self
    }

    pub fn environment(mut self, source: BoxedTaggedSource<Arc<Env>>, evaluator: Eval) -> Self {
        self.environment = Some((source, evaluator));
        self
    }

    pub fn layer(mut self, source: BoxedTaggedSource<I>) -> Self {
        self.config_source_layers.push(source);
        self
    }

    pub fn build<C: RestartRequired + Send + Sync + 'static>(
        self,
        polling_interval: Duration,
    ) -> FetcherResult<ConspiracyConfigFetcher<C>>
    where
        I: TryInto<C, Error: Error + Send + Sync> + Merge,
    {
        self.build_inner(polling_interval, |before, after| {
            if before.restart_required(&after) {
                tracing::warn!("Restart was requested but the config fetcher was built without a restart handler");
            }
        })
    }

    pub fn build_self_restarting<
        C: RestartRequired + Send + Sync + 'static,
        F: Fn() + Send + 'static,
    >(
        self,
        polling_interval: Duration,
        handle_restart_requested: F,
    ) -> FetcherResult<ConspiracyConfigFetcher<C>>
    where
        I: TryInto<C, Error: Error + Send + Sync> + Merge,
    {
        self.build_inner(polling_interval, move |before, after| {
            if before.restart_required(&after) {
                tracing::info!("Requesting restart to apply config changes");
                handle_restart_requested();
            }
        })
    }

    fn build_inner<C: Send + Sync + 'static, F: Fn(Arc<C>, Arc<C>) + Send + 'static>(
        self,
        polling_interval: Duration,
        handle_restart_requested: F,
    ) -> FetcherResult<ConspiracyConfigFetcher<C>>
    where
        I: TryInto<C, Error: Error + Send + Sync> + Merge,
    {
        let config_source_layers: Vec<SourceLayer<I>> = self
            .config_source_layers
            .into_iter()
            .map(|source| SourceLayer {
                // This isn't a very rusty way to do this, because we start with the struct in an
                // invalid state. However, we will never release the struct this way because we
                // immediately fetch from it below. Avoiding that temporary 0 for sake of pureness
                // harms readability.
                etag: Default::default(),
                source,
            })
            .collect();

        let environment = self.environment.map(move |(source, evaluator)| {
            (
                SourceLayer {
                    etag: Default::default(),
                    source,
                },
                Flattener::new(evaluator),
            )
        });

        let mut sources = InnerSource {
            config_source_layers,
            environment,
        };
        let current_config = Arc::new(ArcSwap::from_pointee(try_into_config(
            sources.fetch_latest()?,
        )?));
        let mut inner = InnerFetcherState {
            current_config: current_config.clone(),
            sources,
        };

        std::thread::spawn(move || {
            loop {
                std::thread::sleep(polling_interval);
                if let Some((before, after)) = inner.poll() {
                    handle_restart_requested(before, after);
                }
            }
        });

        Ok(ConspiracyConfigFetcher::new(current_config))
    }
}

fn try_into_config<C, I>(intermediate: I) -> FetcherResult<C>
where
    I: TryInto<C>,
    I::Error: Error + Send + Sync + 'static,
{
    intermediate.try_into().map_err(|error| {
        tracing::error!(%error, "Failed to convert intermediate config");
        Box::new(error) as _
    })
}
