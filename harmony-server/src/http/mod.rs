use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::{
    http::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router, Server,
};
use colored::Colorize;
use log::{debug, error, info};

use crate::{
    cmd::{BackupArgs, HttpArgs},
    data::AppData,
    http::{
        auth::auth_middleware,
        routes::{is_sync_open, resume_open_sync},
    },
    paths::Paths,
};

use self::{
    routes::{begin_sync, finalize_sync, healthcheck, request_access_token, send_file, snapshot},
    state::HttpState,
};

mod auth;
mod errors;
mod routes;
mod state;

pub async fn launch(
    http_args: HttpArgs,
    backup_args: BackupArgs,
    app_data: AppData,
    paths: Paths,
) -> Result<()> {
    let HttpArgs { addr, port } = http_args;

    let state = HttpState::new(backup_args, app_data, paths);

    let app = Router::new()
        .route("/snapshot", post(snapshot))
        .route("/sync/is-open", get(is_sync_open))
        .route("/sync/begin", post(begin_sync))
        .route("/sync/resume", post(resume_open_sync))
        .route("/sync/finalize", post(finalize_sync))
        .route("/sync/file", post(send_file))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        // Routes below can be accessed without authentication
        .route("/request-access-token", post(request_access_token))
        .route("/healthcheck", get(healthcheck))
        .layer(middleware::from_fn(log_errors))
        .with_state(state);

    info!("Listening on {addr}:{port}...");

    Server::bind(&SocketAddr::from((addr, port)))
        .serve(app.into_make_service())
        .await
        .context("HTTP server crashed")
}

async fn log_errors<B>(request: Request<B>, next: Next<B>) -> Response {
    let path = request.uri().path().to_owned();

    let res = next.run(request).await;

    if !res.status().is_success() {
        error!(
            "{} {}",
            res.status().as_u16().to_string().bright_red(),
            path.bright_yellow()
        );
    } else {
        debug!(
            "{} {}",
            res.status().as_u16().to_string().bright_green(),
            path.bright_cyan()
        );
    }

    res
}
