use std::collections::HashMap;

use anyhow::Context;
use axum::{
    extract::{BodyStream, Query, State},
    Json,
};
use filetime::FileTime;
use futures_util::StreamExt;
use harmony_differ::{
    diffing::Diff,
    snapshot::{make_snapshot, SnapshotFileMetadata, SnapshotOptions, SnapshotResult},
};
use log::error;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};

use crate::{handle_err, throw_err};

use super::{
    errors::HttpResult,
    state::{HttpState, OpenSync},
};

pub async fn healthcheck() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
pub struct RequestAccessTokenPayload {
    secret_password: String,
    device_name: String,
}

pub async fn request_access_token(
    State(state): State<HttpState>,
    Json(payload): Json<RequestAccessTokenPayload>,
) -> HttpResult<Json<String>> {
    let mut app_data = state.app_data.write().await;

    let RequestAccessTokenPayload {
        secret_password,
        device_name,
    } = payload;

    if secret_password != state.backup_args.secret {
        throw_err!(BAD_REQUEST, "Invalid secret password provided");
    }

    let access_token = app_data.create_access_token(device_name).clone();

    if let Err(err) = app_data.save(&state.paths.app_data_file()).await {
        error!("Failed to save data file: {err:?}");
        throw_err!(INTERNAL_SERVER_ERROR, "Failed to save app data file");
    }

    Ok(Json(access_token.token().to_owned()))
}

#[derive(Deserialize)]
pub struct SnapshotParams {
    slot_name: String,
    snapshot_options: SnapshotOptions,
}

pub async fn snapshot(
    State(state): State<HttpState>,
    Json(payload): Json<SnapshotParams>,
) -> HttpResult<Json<SnapshotResult>> {
    let SnapshotParams {
        slot_name,
        snapshot_options,
    } = payload;

    // This block contains quick, locking computing
    // After this block we can do the actual transfer without worrying about locking a concurrent request
    let path = {
        let slot = state
            .slots
            .get(&slot_name)
            .context("Provided slot was not found")
            .map_err(handle_err!(NOT_FOUND))?
            .read()
            .await;

        if slot.open_sync.is_some() {
            throw_err!(
                FORBIDDEN,
                "A synchronization is already opened for the provided slot"
            );
        }

        state.paths.slot_content_dir(&slot.infos)
    };

    make_snapshot(path, |_| {}, &snapshot_options)
        .await
        .map(Json)
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))
}

#[derive(Deserialize)]
pub struct BeginSyncParams {
    slot_name: String,
    diff: Diff,
}

#[derive(Serialize)]
pub struct SyncInfos {
    sync_token: String,
    transfer_file_ids: HashMap<String, String>,
    tranfer_size: u64,
}

pub async fn begin_sync(
    State(state): State<HttpState>,
    Json(begin_sync_params): Json<BeginSyncParams>,
) -> HttpResult<Json<SyncInfos>> {
    let BeginSyncParams { slot_name, diff } = begin_sync_params;

    let mut slot = state
        .slots
        .get(&slot_name)
        .context("Provided slot was not found")
        .map_err(handle_err!(NOT_FOUND))?
        .write()
        .await;

    if slot.open_sync.is_some() {
        throw_err!(
            FORBIDDEN,
            "A synchronization is already opened for the provided slot"
        );
    }

    let open_sync = OpenSync::new(diff)?;

    fs::create_dir(state.paths.slot_transfer_dir(&slot.infos, &open_sync.id))
        .await
        .context("Failed to create the synchronization directory")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    fs::create_dir(state.paths.slot_pending_dir(&slot.infos, &open_sync.id))
        .await
        .context("Failed to create the pending transfers directory")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    fs::create_dir(state.paths.slot_completion_dir(&slot.infos, &open_sync.id))
        .await
        .context("Failed to create the complete transfers directory")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    let slot_files_dir = state.paths.slot_content_dir(&slot.infos);

    for relative_path in &open_sync.diff_ops.delete_files {
        fs::remove_file(slot_files_dir.join(relative_path))
            .await
            .with_context(|| format!("Failed to remove file at '{relative_path}'"))
            .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
    }

    for relative_path in &open_sync.diff_ops.delete_empty_dirs {
        fs::remove_dir(slot_files_dir.join(relative_path))
            .await
            .with_context(|| format!("Failed to remove directory at '{relative_path}'"))
            .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
    }

    let sync_infos = SyncInfos {
        sync_token: open_sync.token.to_owned(),

        transfer_file_ids: open_sync
            .files
            .iter()
            .map(|(id, (relative_path, _))| (id.clone(), relative_path.clone()))
            .collect(),

        tranfer_size: open_sync
            .diff_ops
            .send_files
            .iter()
            .map(|(_, mt)| mt.size)
            .sum(),
    };

    // This must come last, otherwise we have a begin synchronization even if we didn't go to the end of its preparation
    slot.open_sync = Some(open_sync);

    Ok(Json(sync_infos))
}

#[derive(Deserialize)]
pub struct IsSyncOpenParams {
    slot_name: String,
}

pub async fn is_sync_open(
    State(state): State<HttpState>,
    Json(payload): Json<IsSyncOpenParams>,
) -> HttpResult<Json<bool>> {
    let IsSyncOpenParams { slot_name } = payload;

    let slot = state
        .slots
        .get(&slot_name)
        .context("Provided slot was not found")
        .map_err(handle_err!(NOT_FOUND))?
        .read()
        .await;

    Ok(Json(slot.open_sync.is_some()))
}

#[derive(Deserialize)]
pub struct ResumeOpenSyncParams {
    slot_name: String,
}

pub async fn resume_open_sync(
    State(state): State<HttpState>,
    Json(payload): Json<ResumeOpenSyncParams>,
) -> HttpResult<Json<SyncInfos>> {
    let ResumeOpenSyncParams { slot_name } = payload;

    let mut slot = state
        .slots
        .get(&slot_name)
        .context("Provided slot was not found")
        .map_err(handle_err!(NOT_FOUND))?
        .write()
        .await;

    let slot_infos = slot.infos.clone();

    let Some(open_sync) = slot.open_sync.as_mut() else {
        throw_err!(
            CONFLICT,
            "No synchronization is currently open for the provided slot"
        )
    };

    let sync_token = open_sync.regenerate_access_token();

    let mut remaining_files = HashMap::new();

    for (id, data) in &open_sync.files {
        if !state
            .paths
            .slot_completion_dir(&slot_infos, &open_sync.id)
            .join(id)
            .exists()
        {
            remaining_files.insert(id.clone(), data.clone());
        }

        let tmp_path = state
            .paths
            .slot_transfer_dir(&slot_infos, &open_sync.id)
            .join(id);

        if tmp_path.exists() {
            fs::remove_file(&tmp_path)
                .await
                .with_context(|| {
                    format!(
                        "Failed to remove partially transferred file at '{}'",
                        tmp_path.display()
                    )
                })
                .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
        }
    }

    Ok(Json(SyncInfos {
        sync_token,
        transfer_file_ids: remaining_files
            .iter()
            .map(|(id, (relative_path, _))| (id.clone(), relative_path.clone()))
            .collect(),
        tranfer_size: open_sync
            .diff_ops
            .send_files
            .iter()
            .filter(|(id, _)| remaining_files.contains_key(id))
            .map(|(_, mt)| mt.size)
            .sum(),
    }))
}

#[derive(Deserialize)]
pub struct SyncFinalizationParams {
    slot_name: String,
    sync_token: String,
}

pub async fn finalize_sync(
    State(state): State<HttpState>,
    Json(payload): Json<SyncFinalizationParams>,
) -> HttpResult<Json<()>> {
    let SyncFinalizationParams {
        slot_name,
        sync_token,
    } = payload;

    let mut slot = state
        .slots
        .get(&slot_name)
        .context("Provided slot was not found")
        .map_err(handle_err!(NOT_FOUND))?
        // Getting an exclusive access right now is very important as it ensures that no
        // other finalization process can happen simultaneously, which would be destructive
        .write()
        .await;

    let open_sync = slot
        .open_sync
        .as_ref()
        .context("No synchronization is currently open for this slot")
        .map_err(handle_err!(NOT_FOUND))?;

    if open_sync.token != sync_token {
        throw_err!(
            BAD_REQUEST,
            "Provided synchronization token does not match currently open sync."
        );
    }

    let complete_dir = state.paths.slot_completion_dir(&slot.infos, &open_sync.id);

    for (relative_path, (id, _)) in &open_sync.files {
        let marker_path = complete_dir.join(id);

        if !marker_path.is_file() {
            throw_err!(
                BAD_REQUEST,
                format!("File '{relative_path}' has not been transferred yet!")
            );
        }

        fs::remove_file(&marker_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to remove marker file at '{}'",
                    marker_path.display()
                )
            })
            .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
    }

    let slot_files_dir = state.paths.slot_content_dir(&slot.infos);

    for relative_path in &open_sync.diff_ops.create_dirs {
        fs::create_dir(slot_files_dir.join(relative_path))
            .await
            .with_context(|| format!("Failed to create folder at '{relative_path}'"))
            .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
    }

    fs::remove_dir(state.paths.slot_pending_dir(&slot.infos, &open_sync.id))
        .await
        .context("Failed to remove the pending transfers directory")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    fs::remove_dir(&complete_dir)
        .await
        .context("Failed to remove the complete transfers directory")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    fs::remove_dir(state.paths.slot_transfer_dir(&slot.infos, &open_sync.id))
        .await
        .context("Failed to remove the slot directory")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    slot.open_sync = None;

    Ok(Json(()))
}

#[derive(Deserialize)]
pub struct SendFileParams {
    slot_name: String,
    sync_id: String,
    path: String,
}

pub async fn send_file(
    Query(params): Query<SendFileParams>,
    State(state): State<HttpState>,
    mut stream: BodyStream,
) -> HttpResult<Json<()>> {
    let SendFileParams {
        slot_name,
        sync_id,
        path,
    } = params;

    // This block contains quick, locking computing
    // After this block we can do the actual transfer without worrying about locking a concurrent request
    let (tmp_path, file_id, metadata, slot_infos) = {
        let slot = state
            .slots
            .get(&slot_name)
            .context("Provided slot was not found")
            .map_err(handle_err!(NOT_FOUND))?
            .read()
            .await;

        let open_sync = slot
            .open_sync
            .as_ref()
            .context("No synchronization is currently open for this slot")
            .map_err(handle_err!(NOT_FOUND))?;

        if open_sync.token != sync_id {
            throw_err!(
                BAD_REQUEST,
                "Provided synchronization token does not match currently open sync."
            );
        }

        let (file_id, metadata) = open_sync
            .files
            .get(&path)
            .ok_or("Provided file was not found in the current synchronization process")
            .map_err(handle_err!(BAD_REQUEST))?;

        let tmp_path = state
            .paths
            .slot_pending_dir(&slot.infos, &sync_id)
            .join(file_id);

        (tmp_path, file_id.clone(), *metadata, slot.infos.clone())
    };

    if tmp_path.is_file() {
        fs::remove_file(&tmp_path)
            .await
            .context("Temporary file already exists but it could not be deleted")
            .map_err(handle_err!(BAD_REQUEST))?;
    }

    let mut tmp_file = File::create(&tmp_path)
        .await
        .context("Failed to create a temporary file")
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    let mut written = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
        written += chunk.len();

        tmp_file
            .write_all(&chunk)
            .await
            .context("Failed to write to temporary file")
            .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;
    }

    let SnapshotFileMetadata {
        last_modif_date_s,
        last_modif_date_ns,
        size,
    } = metadata;

    if u64::try_from(written).unwrap() != size {
        throw_err!(
            BAD_REQUEST,
            "Provided size does not match transmitted content"
        );
    }

    let tmp_path_bis = tmp_path.clone();

    tokio::task::spawn_blocking(move || {
        filetime::set_file_mtime(
            tmp_path_bis,
            FileTime::from_unix_time(last_modif_date_s as i64, last_modif_date_ns),
        )
        .context("Failed to set modification time")
    })
    .await
    .context("Failed to run modification time setter")
    .map_err(handle_err!(INTERNAL_SERVER_ERROR))?
    .context("Failed to run modification time setter")
    .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    // Move file to its destination

    let final_path = state.paths.slot_content_dir(&slot_infos).join(&path);

    fs::rename(&tmp_path, &final_path)
        .await
        .with_context(|| {
            format!(
                "Failed to move complete file '{path}' to '{}'",
                final_path.display()
            )
        })
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    // Create completion marker file

    let marker_path = &state
        .paths
        .slot_completion_dir(&slot_infos, &sync_id)
        .join(&file_id);

    fs::rename(&tmp_path, &marker_path)
        .await
        .with_context(|| {
            format!(
                "Failed to create transfer completion marker file at '{}'",
                marker_path.display()
            )
        })
        .map_err(handle_err!(INTERNAL_SERVER_ERROR))?;

    Ok(Json(()))
}
