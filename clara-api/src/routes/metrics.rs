use actix_web::{web, HttpResponse};

use crate::handlers::AppState;

pub async fn metrics(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(state.prometheus_handle.render())
}
