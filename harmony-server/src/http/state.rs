use harmony_differ::{
    diffing::{Diff, DiffApplyOps},
    snapshot::SnapshotFileMetadata,
};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    cmd::BackupArgs,
    data::{generate_id, AppData},
    paths::{is_relative_linear_path, Paths},
    throw_err,
};

use super::errors::HttpResult;

#[derive(Clone)]
pub struct HttpState {
    pub backup_args: Arc<RwLock<BackupArgs>>,
    pub paths: Arc<RwLock<Paths>>,
    pub app_data: Arc<RwLock<AppData>>,
    pub open_syncs: Arc<RwLock<HashMap<String, Option<OpenSync>>>>,
}

impl HttpState {
    pub fn new(args: BackupArgs, app_data: AppData, paths: Paths) -> Self {
        Self {
            open_syncs: Arc::new(RwLock::new(
                args.slots
                    .iter()
                    .map(|slot| (slot.to_owned(), None))
                    .collect(),
            )),

            backup_args: Arc::new(RwLock::new(args)),
            paths: Arc::new(RwLock::new(paths)),
            app_data: Arc::new(RwLock::new(app_data)),
        }
    }
}

pub struct OpenSync {
    pub sync_token: String,
    pub diff: Diff,
    pub diff_ops: DiffApplyOps,
    pub files: HashMap<String, (String, SnapshotFileMetadata)>,
}

impl OpenSync {
    pub fn new(diff: Diff) -> HttpResult<Self> {
        let diff_ops = diff.ops();

        Ok(Self {
            sync_token: generate_id(),
            files: diff_ops
                .send_files
                .into_iter()
                .map(|(relative_path, mt)| {
                    if is_relative_linear_path(Path::new(&relative_path)) {
                        throw_err!(BAD_REQUEST, format!("Path is trying to escape or contains '.' / '..' components: {relative_path}"));
                    }

                    Ok((relative_path, (generate_id(), mt)))
                })
                .collect::<Result<_, _>>()?,
            diff_ops: diff.ops(),
            diff,
        })
    }
}
