use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use clara_core::ClaraError;
use clara_session::ManagerError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// API error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub error_type: String,
    pub details: String,
    pub code: u16,
}

/// Wrapper for converting ClaraError to HTTP responses
#[derive(Debug)]
pub struct ApiError {
    pub inner: ClaraError,
}

impl ApiError {
    pub fn new(error: ClaraError) -> Self {
        Self { inner: error }
    }

    pub fn status_code(&self) -> StatusCode {
        match self.inner.status_code() {
            400 => StatusCode::BAD_REQUEST,
            403 => StatusCode::FORBIDDEN,
            404 => StatusCode::NOT_FOUND,
            409 => StatusCode::CONFLICT,
            429 => StatusCode::TOO_MANY_REQUESTS,
            500 => StatusCode::INTERNAL_SERVER_ERROR,
            504 => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn response(&self) -> ApiErrorResponse {
        ApiErrorResponse {
            error: self.inner.to_string(),
            error_type: self.inner.error_type(),
            details: self.inner.to_string(),
            code: self.inner.status_code(),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self) -> HttpResponse {
        let error_response = self.response();
        HttpResponse::build(self.status_code()).json(error_response)
    }
}

impl From<ClaraError> for ApiError {
    fn from(error: ClaraError) -> Self {
        Self { inner: error }
    }
}

impl From<ManagerError> for ApiError {
    fn from(error: ManagerError) -> Self {
        let clara_error = match error {
            ManagerError::Store(store_err) => match store_err {
                clara_session::StoreError::NotFound(id) => {
                    ClaraError::SessionNotFound(id)
                }
                clara_session::StoreError::AlreadyExists(id) => {
                    ClaraError::SessionAlreadyExists(id)
                }
                clara_session::StoreError::InvalidState => ClaraError::Internal("Invalid session state".to_string()),
                clara_session::StoreError::LockPoisoned => ClaraError::LockPoisoned,
            },
            ManagerError::UserSessionLimitExceeded => ClaraError::UserSessionLimitExceeded,
            ManagerError::GlobalSessionLimitExceeded => ClaraError::GlobalSessionLimitExceeded,
            ManagerError::SessionTerminated => ClaraError::SessionTerminated,
            ManagerError::SessionNotFound => ClaraError::SessionNotFound("Session not found".to_string()),
            ManagerError::WrongSessionType { expected, actual } => {
                ClaraError::ValidationError(format!("Expected {} session, got {}", expected, actual))
            }
        };
        Self { inner: clara_error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_from_clara_error() {
        let clara_err = ClaraError::SessionNotFound("sess-123".to_string());
        let api_err = ApiError::from(clara_err);

        assert_eq!(api_err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_api_error_response() {
        let clara_err = ClaraError::ValidationError("bad input".to_string());
        let api_err = ApiError::from(clara_err);
        let response = api_err.response();

        assert_eq!(response.code, 400);
        assert_eq!(response.error_type, "ValidationError");
    }
}
