use std::collections::HashMap;

use anyhow::Result;

use crate::{
    diffing::Diff,
    snapshot::{Snapshot, SnapshotItem},
};

pub fn detect_conflicts(truth_source: &Snapshot, to_sync: &Snapshot) -> Result<Diff> {
    todo!()
}

fn detect_one_way_conflicts(base: &Snapshot, to_sync: &Snapshot) {
    // 1. for every item in any source, items shouldn't be deleted in "to_sync" if it's more recent than the deletion
    // 2. for every item in any source, items shouldn't be modified in "to_sync" if it's more recent on the truth side

    let to_sync = to_sync
        .items
        .iter()
        .map(|item| (&item.relative_path, item.metadata))
        .collect::<HashMap<_, _>>();

    for SnapshotItem {
        relative_path,
        metadata,
    } in &base.items
    {}

    todo!()
}
