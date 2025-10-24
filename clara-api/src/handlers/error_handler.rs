use actix_web::HttpResponse;
use crate::models::ApiErrorResponse;

/// Generic error handler
pub fn handle_error(error: &str) -> HttpResponse {
    let response = ApiErrorResponse {
        error: error.to_string(),
        error_type: "InternalError".to_string(),
        details: error.to_string(),
        code: 500,
    };
    HttpResponse::InternalServerError().json(response)
}
