use anyhow::{anyhow, Result};
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
}

impl<'a> Iterator for FallibleEntryFilter<'a> {
    type Item = Result<Result<DirEntry>, walkdir::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.iter.next()?;

            match entry {
                Ok(entry) => match (self.filter)(&entry) {
                    Ok(should_keep) => {
                        if should_keep {
                            break Some(Ok(Ok(entry)));
                        } else {
                            match entry.metadata() {
                                Ok(mt) => {
                                    if mt.is_dir() {
                                        self.iter.skip_current_dir();
                                    }

                                    continue;
                                }
                                Err(err) => {
                                    break Some(Ok(Err(anyhow!(
                                        "Failed to get metadata for filtered item '{}': {}",
                                        entry.path().display(),
                                        err
                                    ))))
                                }
                            }
                        }
                    }
                    Err(err) => break Some(Ok(Err(err))),
                },
                Err(err) => break Some(Err(err)),
            }
        }
    }
}
