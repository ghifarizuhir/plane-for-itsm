use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug)]
pub struct AppError(pub anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": self.0.to_string()}))).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
