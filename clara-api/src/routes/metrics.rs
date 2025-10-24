use actix_web::HttpResponse;

pub async fn metrics() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body("# No metrics yet")
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.route("/metrics", actix_web::web::get().to(metrics));
}
