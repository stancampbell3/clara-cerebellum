use actix_web::HttpResponse;

pub async fn metrics() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body("# No metrics yet")
}
