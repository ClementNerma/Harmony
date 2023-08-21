use axum::{
    extract::State,
    headers::{authorization::Bearer, Authorization},
    http::Request,
    middleware::Next,
    response::Response,
    TypedHeader,
};

use crate::throw_err;

use super::{errors::HttpError, state::HttpState};

pub async fn auth_middleware<B>(
    TypedHeader(Authorization(bearer_token)): TypedHeader<Authorization<Bearer>>,
    State(state): State<HttpState>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, HttpError> {
    authenticate(bearer_token.token(), &state).await?;
    Ok(next.run(request).await)
}

async fn authenticate(bearer_token: &str, state: &HttpState) -> Result<(), HttpError> {
    let mut state = state.app_data.write().await;

    if state.get_access_token(bearer_token).is_none() {
        throw_err!(FORBIDDEN, "Invalid access token provided");
    }

    Ok(())
}
