use anyhow::{Context, Result};
use walkdir::{DirEntry, WalkDir};

pub struct FallibleEntryFilter<'a> {
    iter: walkdir::IntoIter,

    #[allow(clippy::type_complexity)]
    filter: Box<dyn Fn(&DirEntry) -> Result<bool> + Send + Sync + 'a>,
}

impl<'a> FallibleEntryFilter<'a> {
    pub fn new(
        inner: WalkDir,
        filter: impl Fn(&DirEntry) -> Result<bool> + Send + Sync + 'a,
    ) -> Self {
        Self {
            iter: inner.into_iter(),
            filter: Box::new(filter),
        }
    }

    fn iter_next(&mut self) -> Result<Option<DirEntry>> {
        loop {
            let Some(entry) = self.iter.next() else {
                return Ok(None);
            };

            let entry = entry.context("Failed to read next directory entry")?;

            if (self.filter)(&entry)? {
                break Ok(Some(entry));
            }

            let mt = entry.metadata().with_context(|| {
                format!(
                    "Failed to get metadata for filtered item '{}'",
                    entry.path().display()
                )
            })?;

            if mt.is_dir() {
                self.iter.skip_current_dir();
            }
        }
    }
}

impl<'a> Iterator for FallibleEntryFilter<'a> {
    type Item = Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter_next().transpose()
    }
}
