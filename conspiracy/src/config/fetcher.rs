use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use arc_swap::ArcSwap;
use conspiracy_theories::config::ConfigFetcher;

use crate::flight::{Evaluator, Flattenable, Flattener, Merge};

pub type BoxedTaggedSource<T> = Box<dyn TaggedSource<T> + Send>;

struct SourceLayer<T> {
    etag: SourceEtag,
    source: BoxedTaggedSource<T>,
}

impl<T> SourceLayer<T> {
    fn is_fresh(&self) -> bool {
        self.source.is_fresh(self.etag)
    }

    fn fetch(&mut self) -> Result<T, Box<dyn Error + Send + Sync>> {
        let (new_etag, config) = self.source.fetch()?;
        self.etag = new_etag;
        Ok(config)
    }
}

// T config type I intermediate config type (e.g. serde_json::Value, toml::Value, etc)
struct InnerFetcherState<T, I, E: Evaluator> {
    current_config: Arc<ArcSwap<T>>,
    config_source_layers: Vec<SourceLayer<I>>,
    environment: Option<(SourceLayer<E::Environment>, Flattener<E>)>,
}

impl<T, I: Flattenable + Merge, E: Evaluator<Key = I::Key>> InnerFetcherState<T, I, E> {
    fn poll(&mut self)
    where
        I: Into<T>,
    {
        match self.poll_inner() {
            Ok(updated_config) => {
                // None case means cache is still fresh, safe to ignore.
                if let Some(updated_config) = updated_config {
                    tracing::debug!("Applying updated config or environment");
                    self.current_config.store(Arc::new(updated_config.into()));
                }
            }
            Err(error) => {
                tracing::error!(%error, "Failed to fetch latest config or environment");
            }
        }
    }

    fn poll_inner(&mut self) -> Result<Option<I>, Box<dyn Error + Send + Sync>> {
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

    fn fetch_latest(&mut self) -> Result<I, Box<dyn Error + Send + Sync>> {
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
    ) -> Result<I, Box<dyn Error + Send + Sync>> {
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
    T: Flattenable + Merge + 'static,
    Env: Send + Sync + 'static,
    Eval: Evaluator<Key = T::Key, Environment = Arc<Env>> + Send + 'static,
> ConspiracyConfigFetcherBuilder<T, Env, Eval>
{
    pub fn new(base_source: BoxedTaggedSource<T>) -> Self {
        Self {
            config_source_layers: vec![base_source],
            environment: None,
        }
    }

    pub fn static_environment(mut self, env: Env, evaluator: Eval) -> Self {
        self.environment = Some((
            Box::new(StaticCacheSource {
                config: Arc::new(env),
            }),
            evaluator,
        ));
        self
    }

    pub fn environment(mut self, source: BoxedTaggedSource<Arc<Env>>, evaluator: Eval) -> Self {
        self.environment = Some((source, evaluator));
        self
    }

    pub fn layer(mut self, source: BoxedTaggedSource<T>) -> Self {
        self.config_source_layers.push(source);
        self
    }

    pub fn build<C: Send + Sync + 'static>(
        self,
        polling_interval: Duration,
    ) -> Result<ConspiracyConfigFetcher<C>, Box<dyn Error + Send + Sync>>
    where
        T: Into<C> + Merge,
    {
        let (etag, current_config) = self.config_source_layers[0].fetch()?;
        let current_config = Arc::new(ArcSwap::from_pointee(current_config.into()));
        let config_source_layers: Vec<SourceLayer<T>> = self
            .config_source_layers
            .into_iter()
            .map(|layer| SourceLayer {
                source: layer,
                etag: Default::default(),
            })
            .collect();

        let environment = self.environment.map(move |(source, evaluator)| {
            (SourceLayer { etag, source }, Flattener::new(evaluator))
        });

        let mut inner = InnerFetcherState {
            current_config: current_config.clone(),
            config_source_layers,
            environment,
        };

        std::thread::spawn(move || {
            loop {
                std::thread::sleep(polling_interval);
                inner.poll();
            }
        });

        Ok(ConspiracyConfigFetcher::new(current_config))
    }
}

pub type SourceEtag = u128;

pub trait TaggedSource<T> {
    fn is_fresh(&self, etag: SourceEtag) -> bool;

    fn fetch(&self) -> Result<(SourceEtag, T), Box<dyn std::error::Error + Send + Sync>>;
}

pub struct StaticCacheSource<T> {
    config: T,
}

impl<T: Clone + Send + Sync> TaggedSource<T> for StaticCacheSource<T> {
    fn is_fresh(&self, _etag: SourceEtag) -> bool {
        true
    }

    fn fetch(&self) -> Result<(SourceEtag, T), Box<dyn std::error::Error + Send + Sync>> {
        Ok((Default::default(), self.config.clone()))
    }
}

pub struct DiskSource {
    path: PathBuf,
}

impl DiskSource {
    fn get_etag(&self) -> Option<u128> {
        Some(
            fs::metadata(&self.path)
                .ok()?
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos(),
        )
    }
}

impl<T> TaggedSource<T> for DiskSource {
    fn is_fresh(&self, etag: SourceEtag) -> bool {
        // If we failed to look up data, we can rely upon prior state.
        self.get_etag()
            .is_none_or(|current_etag| current_etag > etag)
    }

    fn fetch(&self) -> Result<(SourceEtag, T), Box<dyn Error + Send + Sync>> {
        todo!()
    }
}
