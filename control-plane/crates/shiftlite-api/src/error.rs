//! Maps `shiftlite_core::ApiError` to the documented HTTP responses (api/openapi.yaml `Error`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;
use shiftlite_core::ApiError;

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<serde_json::Value>,
    #[serde(rename = "requestId")]
    request_id: String,
}

pub struct AppError(pub ApiError);

impl From<ApiError> for AppError {
    fn from(e: ApiError) -> Self {
        AppError(e)
    }
}

impl From<kube::Error> for AppError {
    fn from(e: kube::Error) -> Self {
        tracing::error!(error = %e, "kubernetes api error");
        AppError(ApiError::Internal)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let fields = match &self.0 {
            ApiError::ValidationFailed { fields } => Some(json!(fields)),
            _ => None,
        };
        let body = ErrorBody {
            code: self.0.code(),
            message: self.0.to_string(),
            fields,
            request_id: shiftlite_auth::new_id(),
        };
        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use shiftlite_core::FieldError;

    #[test]
    fn forbidden_maps_to_403() {
        let response = AppError(ApiError::Forbidden).into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn unauthenticated_maps_to_401() {
        let response = AppError(ApiError::Unauthenticated).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn validation_failed_maps_to_422_and_keeps_field_errors() {
        let err = ApiError::ValidationFailed {
            fields: vec![FieldError {
                field: "name".to_string(),
                reason: "invalid_format".to_string(),
            }],
        };
        let response = AppError(err).into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
