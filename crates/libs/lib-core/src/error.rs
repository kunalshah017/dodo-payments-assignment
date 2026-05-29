use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

// region:    --- Error & Result Types

pub type Result<T> = core::result::Result<T, Error>;

/// Standard error response body
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    #[schema(example = "not_found")]
    pub r#type: String,
    #[schema(example = "Resource not found")]
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),

    #[error("Database error")]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            Error::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            Error::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            Error::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Invalid or missing API key".to_string(),
            ),
            Error::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            Error::UnprocessableEntity(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "unprocessable_entity",
                msg.clone(),
            ),
            Error::Internal(err) => {
                tracing::error!(error = %err, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                )
            }
            Error::Database(err) => {
                // Check for unique constraint violations (23505 = unique_violation in PostgreSQL)
                if let sqlx::Error::Database(ref db_err) = err {
                    if db_err.code().as_deref() == Some("23505") {
                        return (
                            StatusCode::CONFLICT,
                            Json(json!({
                                "error": {
                                    "type": "conflict",
                                    "message": "A record with this value already exists",
                                }
                            })),
                        )
                            .into_response();
                    }
                }
                tracing::error!(error = %err, "Database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                )
            }
        };

        let body = json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        });

        (status, Json(body)).into_response()
    }
}

// endregion: --- Error & Result Types
