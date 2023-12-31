use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{atomic::AtomicUsize, Arc},
    time::SystemTime,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use walkdir::WalkDir;

use crate::filter::FallibleEntryFilter;

#[derive(Serialize, Deserialize, Debug)]
pub struct Snapshot {
    pub from_dir: String,
    pub items: Vec<SnapshotItem>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SnapshotItem {
    pub relative_path: String,
    pub metadata: SnapshotItemMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum SnapshotItemMetadata {
    Directory,
    File(SnapshotFileMetadata),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotFileMetadata {
    pub size: u64,
    pub last_modif_date_s: u64,
    pub last_modif_date_ns: u32,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SnapshotOptions {
    pub ignore_paths: Vec<String>,
    pub ignore_names: Vec<String>,
    pub ignore_exts: Vec<String>,
}

impl SnapshotOptions {
    pub fn validate(&self) -> Result<()> {
        for path in &self.ignore_paths {
            if Path::new(path).is_absolute() {
                bail!("Paths to ignore must be relative (got '{path}')",);
            }
        }

        for name in &self.ignore_names {
            if name.contains('/') || name.contains('\\') {
                bail!(
                    "Names to ignore must not contain a path separator (got '{}')",
                    name
                );
            }
        }

        for ext in &self.ignore_exts {
            if ext.contains('.') {
                bail!(
                    "Extensions to ignore must not contain a dot (got '{}')",
                    ext
                );
            }
        }

        Ok(())
    }

    pub fn should_ignore(&self, path: &Path, from_dir: &Path) -> Result<bool> {
        let relative_path = path.strip_prefix(from_dir).unwrap();

        if self
            .ignore_paths
            .iter()
            .any(|c| relative_path.strip_prefix(c).is_ok())
        {
            return Ok(true);
        }

        if self.ignore_names.iter().any(|c| {
            relative_path
                .components()
                .any(|component| component.as_os_str() == OsStr::new(c))
        }) {
            return Ok(true);
        }

        let mt = path.metadata().with_context(|| {
            format!(
                "Failed to get metadata for path: {}",
                relative_path.display()
            )
        })?;

        if mt.is_file() {
            if let Some(ext) = relative_path.extension() {
                if self.ignore_exts.iter().any(|c| OsStr::new(c) == ext) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SnapshotResult {
    pub snapshot: Snapshot,
}

pub async fn make_snapshot(
    from_dir: PathBuf,
    progress: impl Fn(String) + Send + Sync + 'static,
    options: &SnapshotOptions,
) -> Result<SnapshotResult> {
    options.validate()?;

    let total = Arc::new(Mutex::new(AtomicUsize::new(0)));
    let progress = Arc::new(progress);

    let mut items = Vec::new();

    let walker = WalkDir::new(&from_dir).min_depth(1);
    let walker_with_ignores = FallibleEntryFilter::new(walker, |entry| {
        options
            .should_ignore(entry.path(), &from_dir)
            .map(|ignore| !ignore)
    });

    for item in walker_with_ignores {
        let item = item.context("Failed to analyze directory entry")?;

        let from = from_dir.clone();

        let progress = Arc::clone(&progress);
        let total = Arc::clone(&total);

        let path = item.path();

        let item = snapshot_item(path, &from)
            .await
            .with_context(|| format!("Failed analysis on filesystem item: {}", path.display()))?;

        items.push(item);

        let total = total
            .lock()
            .await
            .fetch_add(1, std::sync::atomic::Ordering::Release)
            + 1;

        progress(format!("Analyzed {total} item(s)"));
    }

    let from_dir_str = from_dir.to_str().with_context(|| {
        format!(
            "Provided path contains invalid UTF-8 characters: {}",
            from_dir.display()
        )
    })?;

    Ok(SnapshotResult {
        snapshot: Snapshot {
            from_dir: from_dir_str.to_string(),
            items,
        },
    })
}

async fn snapshot_item(item: &Path, from: &Path) -> Result<SnapshotItem> {
    let metadata = item.metadata()?;

    if metadata.is_symlink() {
        bail!("Symbolc links are unsupported.");
    }

    let metadata = if metadata.is_dir() {
        SnapshotItemMetadata::Directory
    } else if metadata.is_file() {
        let mtime = metadata
            .modified()
            .with_context(|| {
                format!(
                    "Failed to get modification time of file: {}",
                    item.display()
                )
            })?
            .duration_since(SystemTime::UNIX_EPOCH)
            .with_context(|| {
                format!(
                    "Found invalid modification time for file: {}",
                    item.display()
                )
            })?;

        SnapshotItemMetadata::File(SnapshotFileMetadata {
            size: metadata.len(),
            last_modif_date_s: mtime.as_secs(),
            last_modif_date_ns: mtime.subsec_nanos(),
        })
    } else {
        bail!("Unknown item type (not a symlink, file nor directory)");
    };

    let relative_path = item.strip_prefix(from).unwrap();

    let relative_path_str = relative_path.to_str().with_context(|| {
        format!(
            "Relative path contains invalid UTF-8 characters: {}",
            relative_path.display()
        )
    })?;

    Ok(SnapshotItem {
        relative_path: relative_path_str.to_string(),
        metadata,
    })
}
