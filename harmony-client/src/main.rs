#![forbid(unsafe_code)]
#![forbid(unused_must_use)]
#![warn(unused_crate_dependencies)]

mod cmd;
mod logging;

use std::{
    collections::HashMap,
    future::Future,
    path::Path,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use cmd::Args;
use colored::Colorize;
use dialoguer::Confirm;
use futures_util::TryStreamExt;
use gethostname::gethostname;
use harmony_differ::{
    diffing::{Diff, DiffItemModified},
    snapshot::{
        make_snapshot, SnapshotFileMetadata, SnapshotItemMetadata, SnapshotOptions, SnapshotResult,
    },
};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use reqwest::{Body, Client, IntoUrl, RequestBuilder, Url};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::json;
use time::OffsetDateTime;
use tokio::{
    fs::File,
    sync::{Mutex, RwLock},
    task::JoinSet,
    try_join,
};
use tokio_util::codec::{BytesCodec, Decoder};

use crate::logging::PRINT_DEBUG_MESSAGES;

#[tokio::main]
async fn main() {
    if let Err(err) = inner_main().await {
        error!("{err:?}");
        std::process::exit(1);
    }
}

async fn inner_main() -> Result<()> {
    let Args {
        data_dir,
        server_address: address,
        server_secret,
        device_name,
        slot_name,
        verbose,
        max_parallel_transfers,
        ignore_items,
        ignore_exts,
    } = Args::parse();

    if verbose {
        PRINT_DEBUG_MESSAGES.store(true, Ordering::SeqCst);
    }

    debug!("Started.");

    if !data_dir.is_dir() {
        bail!("Provided data directory was not found");
    }

    let url = Url::parse(&address)?;

    // ======================================================= //
    // =
    // = Request an access token
    // =
    // ======================================================= //

    // TODO: store the access token
    debug!("Requesting access token...");

    let device_name = device_name.unwrap_or_else(|| gethostname().to_string_lossy().into_owned());

    let access_token = request_url::<String>(url.join("request-access-token")?, "-", |client| {
        client.json(&json!({
            "secret_password": server_secret,
            "device_name": device_name
        }))
    })
    .await
    .context("Failed to request an access token")?;

    drop(server_secret);

    // ======================================================= //
    // =
    // = Build local and remote snapshots
    // =
    // ======================================================= //

    info!("Building snapshots...");

    let snapshot_optoins = SnapshotOptions {
        ignore_paths: ignore_items
            .iter()
            .filter(|item| Path::new(item).is_absolute())
            .map(|item| item.strip_prefix('/').unwrap().to_string())
            .collect(),

        ignore_names: ignore_items
            .into_iter()
            .filter(|item| !Path::new(item).is_absolute())
            .collect(),

        ignore_exts,
    };

    let multi_progress = MultiProgress::new();

    let local_pb = multi_progress.add(async_spinner());
    let remote_pb =
        multi_progress.add(async_spinner().with_message("Building snapshot on server..."));

    local_pb.enable_steady_tick(Duration::from_millis(150));
    remote_pb.enable_steady_tick(Duration::from_millis(150));

    let (local, remote) = try_join!(
        async_with_spinner(local_pb, |pb| make_snapshot(
            data_dir.clone(),
            pb,
            &snapshot_optoins
        )),
        async_with_spinner(remote_pb, |_| request_url::<SnapshotResult>(
            url.join("snapshot").unwrap(),
            &access_token,
            |client| client.json(&json!({
                "slot_name": slot_name,
                "snapshot_options": snapshot_optoins,
            }))
        ))
    )?;

    for msg in local.debug {
        debug!("[snapshot:local] {msg}");
    }

    for msg in remote.debug {
        debug!("[snapshot:remote] {msg}");
    }

    // ======================================================= //
    // =
    // = Perform snapshots diffing and display
    // =
    // ======================================================= //

    info!("Diffing...");

    let diff = Diff::build(&local.snapshot, &remote.snapshot);

    let Diff {
        added,
        modified,
        type_changed,
        deleted,
    } = &diff;

    let modified = modified
    .iter()
        .filter(|(path, DiffItemModified { prev, new })| {
            let SnapshotFileMetadata {
                size,
                last_modif_date,
                last_modif_date_ns: _,
            } = new;

            if *size != prev.size {
                return true;
            }

            let truncated_timestamp_diff = last_modif_date.abs_diff(prev.last_modif_date);

            if truncated_timestamp_diff <= 1 {
                debug!("Ignoring modified item '{path}' as modification time is no more than 2 seconds.");
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();

    if added.is_empty() && modified.is_empty() && type_changed.is_empty() && deleted.is_empty() {
        success!("Nothing to do!");
        return Ok(());
    }

    if !added.is_empty() {
        info!("Added:");

        for (path, added) in added {
            match added.new {
                SnapshotItemMetadata::Directory => {
                    println!(" {}", format!("{}/", path).bright_green())
                }
                SnapshotItemMetadata::File(m) => println!(
                    " {} {}",
                    path.bright_green(),
                    format!("({})", HumanBytes(m.size)).bright_yellow()
                ),
            }
        }

        println!();
    }

    if !modified.is_empty() {
        info!("Modified:");

        for (path, DiffItemModified { prev, new }) in &modified {
            let how = if prev.size != new.size {
                format!("({} => {})", HumanBytes(prev.size), HumanBytes(new.size))
            } else if prev.last_modif_date != new.last_modif_date
                || prev.last_modif_date_ns != new.last_modif_date_ns
            {
                let prev =
                    OffsetDateTime::from_unix_timestamp(prev.last_modif_date.try_into().unwrap())
                        .unwrap()
                        + Duration::from_nanos(prev.last_modif_date_ns.into());

                let new =
                    OffsetDateTime::from_unix_timestamp(new.last_modif_date.try_into().unwrap())
                        .unwrap()
                        + Duration::from_nanos(new.last_modif_date_ns.into());

                format!("({prev} => {new})")
            } else {
                unreachable!();
            };

            println!("{} {}", path.bright_yellow(), how.bright_yellow());
        }

        println!();
    }

    if !type_changed.is_empty() {
        info!("Type changed:");

        let type_letter = |m: SnapshotItemMetadata| match m {
            SnapshotItemMetadata::Directory => "D",
            SnapshotItemMetadata::File(_) => "F",
        };

        for (path, type_changed) in type_changed {
            let message = format!(
                " {}{} ({} => {})",
                path,
                if matches!(type_changed.new, SnapshotItemMetadata::Directory) {
                    "/"
                } else {
                    ""
                },
                type_letter(type_changed.prev),
                type_letter(type_changed.new)
            );

            println!("{}", message.bright_yellow());
        }

        println!();
    }

    if !deleted.is_empty() {
        info!("Deleted:");

        for (path, deleted) in deleted {
            match deleted.prev {
                SnapshotItemMetadata::Directory => {
                    info!(" {}", format!("{path}/").bright_red())
                }
                SnapshotItemMetadata::File(m) => info!(
                    " {} {}",
                    path.bright_red(),
                    format!("({})", HumanBytes(m.size)).bright_yellow()
                ),
            }
        }

        info!("");
    }

    let diff_ops = diff.ops();

    let transfer_size = diff_ops.send_files.iter().map(|(_, mt)| mt.size).sum();

    info!(
        "Found a total of {} files to transfer, {} files and {} directories to delete for a total of {}",
        diff_ops.send_files.len().to_string().bright_green(),
        diff_ops.delete_files.len().to_string().bright_red(),
        diff_ops.delete_empty_dirs.len().to_string().bright_red(),
        format!(
            "{}",
            HumanBytes(transfer_size)
        )
        .bright_yellow()
    );

    let confirm = Confirm::new()
        .with_prompt("Continue?".bright_blue().to_string())
        .interact()?;

    if !confirm {
        warn!("Transfer was cancelled.");
        std::process::exit(1);
    }

    // ======================================================= //
    // =
    // = Begin synchronization
    // =
    // ======================================================= //

    debug!("Sending diff to server...");

    let sync_infos = request_url::<SyncInfos>(url.join("begin-sync")?, &access_token, |client| {
        client.json(&json!({
            "slot_name": slot_name,
            "diff": diff
        }))
    })
    .await
    .context("Failed to begin synchronization")?;

    let SyncInfos {
        sync_token,
        transfer_file_ids,
    } = sync_infos;

    let mp = MultiProgress::new();

    let pb_msg = Arc::new(RwLock::new(
        mp.add(
            ProgressBar::new(1)
                .with_style(ProgressStyle::with_template("{msg}").unwrap())
                .with_message("Running..."),
        ),
    ));

    let transfer_pb = Arc::new(RwLock::new(
        mp.add(
            ProgressBar::new(transfer_file_ids.len() as u64).with_style(
                ProgressStyle::with_template(
                    "Transferring : [{elapsed_precise}] {prefix} {bar:40} {pos}/{len} files",
                )
                .unwrap(),
            ),
        ),
    ));

    let transfer_size_pb = Arc::new(RwLock::new(
        mp.add(
            ProgressBar::new(transfer_size ).with_style(
                ProgressStyle::with_template(
                    "Transfer size: [{elapsed_precise}] {prefix} {bar:40} {bytes}/{total_bytes} ({binary_bytes_per_sec})",
                )
                .unwrap(),
            ),
        )
    ));

    let errors = Arc::new(Mutex::new(vec![]));

    macro_rules! report_err {
        ($err: expr, $errors: expr, $pb_msg: expr) => {{
            let mut errors = $errors.lock().await;

            errors.push($err);

            let pb = $pb_msg.read().await;

            pb.println($err);
            pb.set_message(format!(
                "Running... (encountered {} error(s))",
                errors.len(),
            ));
        }};
    }

    let mut task_pool = JoinSet::new();

    let max_parallel_transfers =
        max_parallel_transfers.unwrap_or_else(|| std::cmp::min(num_cpus::get(), 8));

    for (relative_path, _) in transfer_file_ids {
        while task_pool.len() > max_parallel_transfers {
            task_pool.join_next().await.unwrap()?;
        }

        let data_dir = data_dir.clone();

        let errors = Arc::clone(&errors);
        let pb_msg = Arc::clone(&pb_msg);
        let transfer_size_pb = Arc::clone(&transfer_size_pb);

        transfer_pb.read().await.inc(1);

        match File::open(data_dir.join(&relative_path)).await {
            Err(err) => {
                report_err!(
                    format!("Failed to open file '{relative_path}' for transfer: {err}"),
                    errors,
                    pb_msg
                );
            }

            Ok(file) => {
                let transfer_size_pb = transfer_size_pb.clone();

                let stream = BytesCodec::new().framed(file).inspect_ok(move |chunk| {
                    let size = chunk.len() as u64;
                    let transfer_size_pb = Arc::clone(&transfer_size_pb);

                    tokio::spawn(async move {
                        transfer_size_pb.read().await.inc(size);
                    });
                });

                // Prepare variables for task closure
                let url = url.join("send-file")?;
                let access_token = access_token.clone();
                let query = json!({
                    "slot_name": slot_name,
                    "sync_token": sync_token,
                    "path": relative_path
                });
                let file_body = Body::wrap_stream(stream);
                let relative_path = relative_path.clone();

                // Send file
                task_pool.spawn(async move {
                    let req = request_url::<()>(url, &access_token, |client| {
                        client.query(&query).body(file_body)
                    });

                    if let Err(err) = req.await {
                        report_err!(
                            format!("Failed to transfer file '{relative_path}': {err}"),
                            errors,
                            pb_msg
                        );
                    }
                });
            }
        }
    }

    while let Some(result) = task_pool.join_next().await {
        result?;
    }

    transfer_pb.write().await.finish_and_clear();
    transfer_size_pb.write().await.finish_and_clear();

    // ======================================================= //
    // =
    // = Finalize synchronization
    // =
    // ======================================================= //

    let errors = errors.lock().await;

    if !errors.is_empty() {
        for error in errors.as_slice() {
            error!("* {error}");
        }

        bail!("{} error(s) occurred.", errors.len());
    }

    info!("Finalization synchronization on the server...");

    request_url::<()>(url.join("finalize-sync")?, &access_token, |client| {
        client.json(&json!({
            "slot_name": slot_name,
            "sync_token": sync_token
        }))
    })
    .await
    .context("Failed to finalize synchronization")?;

    // ======================================================= //
    // =
    // = Done!
    // =
    // ======================================================= //

    success!("Synchronized successfully.");

    Ok(())
}

#[derive(Deserialize)]
pub struct SyncInfos {
    sync_token: String,
    transfer_file_ids: HashMap<String, String>,
}

async fn request_url<T: DeserializeOwned>(
    url: impl IntoUrl,
    access_token: &str,
    with_client: impl FnOnce(RequestBuilder) -> RequestBuilder,
) -> Result<T> {
    let req = Client::new().post(url).bearer_auth(access_token);

    let res = with_client(req)
        .send()
        .await
        .context("HTTP request failed")?;

    if let Err(err) = res.error_for_status_ref() {
        let res_text = res
            .text()
            .await
            .unwrap_or_else(|_| "<failed to get response body as text>".to_string());

        return Err(anyhow!("{err}").context(format!("Response body: {res_text}")));
    }

    let text = res
        .text()
        .await
        .context("Failed to get HTTP response body as JSON")?;

    let res = serde_json::from_str::<T>(&text).with_context(|| {
        format!(
            "Failed to parse HTTP response as JSON: {}",
            text.bright_yellow()
        )
    })?;

    Ok(res)
}

fn async_spinner() -> ProgressBar {
    ProgressBar::new_spinner()
        .with_style(ProgressStyle::with_template("{spinner} [{elapsed_precise}] {msg}").unwrap())
}

async fn async_with_spinner<F: Future<Output = Result<T, E>>, T, E>(
    pb: ProgressBar,
    task: impl FnOnce(Box<dyn Fn(String) + Send + Sync>) -> F,
) -> Result<T, E> {
    let pb_closure = pb.clone();

    let result = task(Box::new(move |msg| pb_closure.set_message(msg))).await;

    pb.set_style(pb.style().tick_chars(&format!(
        " {}",
        match result {
            Ok(_) => '✅',
            Err(_) => '❌',
        }
    )));

    pb.finish();

    result
}
