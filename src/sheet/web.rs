use actix_web::{http::StatusCode, post, web, Responder};
use serde::{Deserialize, Serialize};

use crate::db::SheetId;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(post);
}

#[derive(Serialize, Deserialize)]
enum PostResponse {
    Success { sheet_id: String },

    Failure { error: String },
}

#[post("/")]
async fn post(data: web::Data<crate::AppData>, schema: web::Json<super::Schema>) -> impl Responder {
    match data.db.new_sheet(&schema).await {
        Ok(SheetId(sheet_id)) => web::Json(PostResponse::Success { sheet_id }).customize(),
        Err(_) => web::Json(PostResponse::Failure {
            error: "invalid schema".into(),
        })
        .customize()
        .with_status(StatusCode::BAD_REQUEST),
    }
}
