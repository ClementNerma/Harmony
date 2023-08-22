use crate::snapshot::{Snapshot, SnapshotFileMetadata, SnapshotItem, SnapshotItemMetadata};

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Diff {
    pub added: Vec<(String, DiffItemAdded)>,
    pub modified: Vec<(String, DiffItemModified)>,
    pub type_changed: Vec<(String, DiffItemTypeChanged)>,
    pub deleted: Vec<(String, DiffItemDeleted)>,
}

impl Diff {
    pub fn new(items: Vec<DiffItem>) -> Self {
        let mut added = vec![];
        let mut modified = vec![];
        let mut type_changed = vec![];
        let mut deleted = vec![];

        for item in items {
            match item.status {
                DiffType::Added(i) => added.push((item.path, i)),
                DiffType::Modified(i) => modified.push((item.path, i)),
                DiffType::TypeChanged(i) => type_changed.push((item.path, i)),
                DiffType::Deleted(i) => deleted.push((item.path, i)),
            }
        }

        Self {
            added,
            modified,
            type_changed,
            deleted,
        }
    }

    pub fn build(local: &Snapshot, remote: &Snapshot) -> Self {
        let source_items = build_item_names_hashmap(local);
        let backed_up_items = build_item_names_hashmap(remote);

        let source_items_paths: HashSet<_> = source_items.keys().collect();
        let backed_up_items_paths: HashSet<_> = backed_up_items.keys().collect();

        let mut diff = Vec::with_capacity(source_items.len());

        // debug!("> Building list of new items...");

        diff.extend(
            source_items_paths
                .difference(&backed_up_items_paths)
                .map(|item| DiffItem {
                    path: item.to_string(),
                    status: DiffType::Added(DiffItemAdded {
                        new: source_items.get(*item).unwrap().metadata,
                    }),
                }),
        );

        // debug!("> Building list of deleted items...");

        diff.extend(
            backed_up_items_paths
                .difference(&source_items_paths)
                .map(|item| DiffItem {
                    path: item.to_string(),
                    status: DiffType::Deleted(DiffItemDeleted {
                        prev: backed_up_items.get(*item).unwrap().metadata,
                    }),
                }),
        );

        // debug!("> Building list of modified items...");

        diff.extend(
            local
                .items
                .iter()
                .filter(|item| backed_up_items_paths.contains(&item.relative_path.as_str()))
                .filter_map(|source_item| {
                    let backed_up_item = backed_up_items
                        .get(&source_item.relative_path.as_str())
                        .unwrap();

                    match (source_item.metadata, backed_up_item.metadata) {
                        // Both directories = no change
                        (SnapshotItemMetadata::Directory, SnapshotItemMetadata::Directory) => None,
                        // Source item is directory and backed up item is file or the opposite = type changed
                        (SnapshotItemMetadata::Directory, SnapshotItemMetadata::File { .. })
                        | (SnapshotItemMetadata::File { .. }, SnapshotItemMetadata::Directory) => {
                            Some(DiffItem {
                                path: source_item.relative_path.clone(),
                                status: DiffType::TypeChanged(DiffItemTypeChanged {
                                    prev: backed_up_item.metadata,
                                    new: source_item.metadata,
                                }),
                            })
                        }
                        // Otherwise, compare their metadata to see if something changed
                        (
                            SnapshotItemMetadata::File(source_data),
                            SnapshotItemMetadata::File(backed_up_data),
                        ) => {
                            if source_data == backed_up_data {
                                None
                            } else {
                                Some(DiffItem {
                                    path: source_item.relative_path.clone(),
                                    status: DiffType::Modified(DiffItemModified {
                                        prev: backed_up_data,
                                        new: source_data,
                                    }),
                                })
                            }
                        }
                    }
                }),
        );

        diff.sort_by(|a, b| a.path.cmp(&b.path));

        Self::new(diff)
    }

    pub fn apply_time_granularity(mut self, time_granularity: Duration) -> Self {
        self.modified = self
            .modified
            .into_iter()
            .filter(|(_, DiffItemModified { prev, new })| {
                // Destructuring isn't necessary, but it allows us to ensure we are correctly using every single field of the metadata
                let SnapshotFileMetadata {
                    size,
                    last_modif_date_s,
                    last_modif_date_ns,
                } = new;

                if *size != prev.size {
                    return true;
                }

                let new_modified_at = Duration::from_secs(*last_modif_date_s)
                    + Duration::from_nanos((*last_modif_date_ns).into());

                let prev_modified_at = Duration::from_secs(prev.last_modif_date_s)
                    + Duration::from_nanos(prev.last_modif_date_ns.into());

                let diff_abs = new_modified_at
                    .checked_sub(prev_modified_at)
                    .or_else(|| prev_modified_at.checked_sub(new_modified_at))
                    .unwrap();

                diff_abs <= time_granularity
            })
            .collect();

        self
    }

    pub fn ops(&self) -> DiffApplyOps {
        DiffApplyOps::new(self)
    }
}

#[derive(Serialize, Deserialize)]
pub struct DiffItem {
    pub status: DiffType,
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub enum DiffType {
    Added(DiffItemAdded),
    Modified(DiffItemModified),
    TypeChanged(DiffItemTypeChanged), // File => Dir / Dir => File
    Deleted(DiffItemDeleted),
}

#[derive(Serialize, Deserialize)]
pub struct DiffItemAdded {
    pub new: SnapshotItemMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct DiffItemModified {
    pub prev: SnapshotFileMetadata,
    pub new: SnapshotFileMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct DiffItemTypeChanged {
    pub prev: SnapshotItemMetadata,
    pub new: SnapshotItemMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct DiffItemDeleted {
    pub prev: SnapshotItemMetadata,
}

fn build_item_names_hashmap(snapshot: &Snapshot) -> HashMap<&str, &SnapshotItem> {
    snapshot
        .items
        .iter()
        .map(|item| (item.relative_path.as_str(), item))
        .collect::<HashMap<_, _>>()
}

#[derive(Serialize, Deserialize)]
pub struct DiffApplyOps {
    pub create_dirs: Vec<String>,
    pub send_files: Vec<(String, SnapshotFileMetadata)>,
    pub delete_files: Vec<String>,
    pub delete_empty_dirs: Vec<String>,
}

impl DiffApplyOps {
    pub fn new(diff: &Diff) -> Self {
        let Diff {
            added,
            modified,
            type_changed,
            deleted,
        } = diff;

        Self {
            // Compute directories to create
            create_dirs: sort_rev_in_place(
                added
                    .iter()
                    .filter_map(|(path, DiffItemAdded { new })| match new {
                        SnapshotItemMetadata::Directory => Some(path),
                        SnapshotItemMetadata::File(_) => None,
                    })
                    .chain(type_changed.iter().filter_map(
                        |(path, DiffItemTypeChanged { prev: _, new })| match new {
                            SnapshotItemMetadata::Directory => Some(path),
                            SnapshotItemMetadata::File(_) => None,
                        },
                    ))
                    .cloned()
                    .collect(),
            ),

            // Compute files to send
            send_files: added
                .iter()
                .filter_map(|(path, DiffItemAdded { new })| match new {
                    SnapshotItemMetadata::Directory => None,
                    SnapshotItemMetadata::File(mt) => Some((path.clone(), *mt)),
                })
                .chain(
                    modified
                        .iter()
                        .map(|(path, DiffItemModified { prev: _, new })| (path.clone(), *new)),
                )
                .chain(type_changed.iter().filter_map(
                    |(path, DiffItemTypeChanged { prev: _, new })| match new {
                        SnapshotItemMetadata::Directory => None,
                        SnapshotItemMetadata::File(mt) => Some((path.clone(), *mt)),
                    },
                ))
                .collect(),

            // Compute files to delete
            delete_files: deleted
                .iter()
                .map(|(path, DiffItemDeleted { prev })| (path, prev))
                .chain(
                    type_changed
                        .iter()
                        .map(|(path, DiffItemTypeChanged { prev, new: _ })| (path, prev)),
                )
                .filter_map(|(path, mt)| match mt {
                    SnapshotItemMetadata::Directory => None,
                    SnapshotItemMetadata::File(_) => Some(path.clone()),
                })
                .collect(),

            // Compute directories to delete
            delete_empty_dirs: sort_rev_in_place(
                deleted
                    .iter()
                    .map(|(path, DiffItemDeleted { prev })| (path, prev))
                    .chain(
                        type_changed
                            .iter()
                            .map(|(path, DiffItemTypeChanged { prev, new: _ })| (path, prev)),
                    )
                    .rev()
                    .filter_map(|(path, mt)| match mt {
                        SnapshotItemMetadata::Directory => Some(path.clone()),
                        SnapshotItemMetadata::File(_) => None,
                    })
                    .collect(),
            ),
        }
    }
}

fn sort_rev_in_place<T: Ord>(mut vec: Vec<T>) -> Vec<T> {
    vec.sort_by(|a, b| b.cmp(a));
    vec
}
