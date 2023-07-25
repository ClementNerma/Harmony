use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use colored::Colorize;
use log::error;
use serde::Serialize;

pub type HttpResult<T> = Result<T, HttpError>;

#[derive(Serialize)]
pub struct HttpError {
    http_code: u16,
    http_name: String,
    message: String,
    #[serde(skip)]
    code: StatusCode,
}

impl HttpError {
    pub fn create_and_log(code: StatusCode, message: impl Into<String>) -> Self {
        let message = message.into();

        error!("HTTP request failed: {}", message.bright_yellow());

        Self {
            http_code: code.as_u16(),
            http_name: code.to_string(),
            code,
            message,
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.code, self.message).into_response()
    }
}

#[macro_export]
macro_rules! server_err {
    ($variant: ident, $msg: expr) => {{
        $crate::http::errors::HttpError::create_and_log(::axum::http::StatusCode::$variant, $msg)
    }};
}

#[macro_export]
macro_rules! handle_err {
    ($variant: ident) => {
        |err| $crate::server_err!($variant, format!("{err:?}"))
    };
}

#[macro_export]
macro_rules! throw_err {
    ($variant: ident, $err: expr) => {
        return Err($crate::server_err!($variant, $err))
    };
}
