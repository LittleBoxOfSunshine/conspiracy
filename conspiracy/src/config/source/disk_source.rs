use std::{error::Error, fs, path::PathBuf, time::UNIX_EPOCH};

use super::{SourceEtag, TaggedSource};

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
