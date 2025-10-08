use super::{SourceEtag, TaggedSource};

pub struct StaticCacheSource<T> {
    config: T,
}

impl<T> StaticCacheSource<T> {
    pub fn new(config: T) -> Self {
        Self { config }
    }
}

impl<T: Clone + Send + Sync> TaggedSource<T> for StaticCacheSource<T> {
    fn is_fresh(&self, _etag: SourceEtag) -> bool {
        true
    }

    fn fetch(&self) -> Result<(SourceEtag, T), Box<dyn std::error::Error + Send + Sync>> {
        Ok((Default::default(), self.config.clone()))
    }
}
