pub mod disk_source;
pub mod static_source;

pub type SourceEtag = u128;

pub trait TaggedSource<T> {
    fn is_fresh(&self, etag: SourceEtag) -> bool;

    fn fetch(&self) -> Result<(SourceEtag, T), Box<dyn std::error::Error + Send + Sync>>;
}
