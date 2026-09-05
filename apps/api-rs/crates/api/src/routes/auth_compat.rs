// crates/api/src/routes/auth_compat.rs
//! Paritas Django `/auth/*` compat (`apps/api/plane/authentication/urls.py`).
//! Rust tidak memakai CSRF (pengganti: Origin/Referer check di middleware),
//! jadi token selalu string kosong — caller hanya meneruskannya sebagai
//! header `X-CSRFTOKEN` yang diabaikan server.

use axum::Json;
use serde_json::{json, Value};

pub fn csrf_token_value() -> Value {
    json!({"csrf_token": ""})
}

/// GET /auth/get-csrf-token/ — paritas `CSRFTokenEndpoint` (`common.py:28`).
pub async fn csrf_token() -> Json<Value> {
    Json(csrf_token_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrf_token_shape() {
        let v = csrf_token_value();
        assert_eq!(v, serde_json::json!({"csrf_token": ""}));
    }
}
