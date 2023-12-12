use actix_web::{get, http::StatusCode, post, web, Responder};
use serde::{Deserialize, Serialize};

use crate::db::SheetId;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(post).service(post_sheetid).service(get_sheetid);
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
enum PostResponse {
    Success { sheet_id: String },

    Failure { error: String },
}

#[post("")]
async fn post(
    data: web::Data<crate::AppData>,
    schema: Option<web::Json<super::Schema>>,
) -> impl Responder {
    if let Some(schema) = schema {
        match data.db.new_sheet(&schema).await {
            Ok(sheet_id) => {
                return web::Json(PostResponse::Success {
                    sheet_id: sheet_id.inner().into(),
                })
                .customize()
            }
            Err(why) => {
                log::warn!("error when servicing post: {why}");
            }
        }
    }

    web::Json(PostResponse::Failure {
        error: "invalid schema".into(),
    })
    .customize()
    .with_status(StatusCode::BAD_REQUEST)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
enum PostSheetIdResponse {
    Success {},

    Failure { error: String },
}

#[post("/{sheetid}")]
async fn post_sheetid(
    data: web::Data<crate::AppData>,
    sheetid: Option<web::Path<SheetId>>,
    cell: Option<web::Json<super::Cell>>,
) -> impl Responder {
    let Some(sheetid) = sheetid else {
        return web::Json(PostSheetIdResponse::Failure {
            error: "invalid sheetid".into(),
        })
        .customize()
        .with_status(StatusCode::BAD_REQUEST);
    };

    let Some(cell) = cell else {
        return web::Json(PostSheetIdResponse::Failure {
            error: "invalid request body".into(),
        })
        .customize()
        .with_status(StatusCode::BAD_REQUEST);
    };

    match data.db.insert_cell(&sheetid, &cell).await {
        Ok(()) => web::Json(PostSheetIdResponse::Success {}).customize(),
        Err(why) => web::Json(PostSheetIdResponse::Failure {
            error: why.to_string(),
        })
        .customize()
        .with_status(StatusCode::BAD_REQUEST),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
enum GetSheetIdResponse {
    Success(super::SheetContent),
    Failure { error: String },
}

#[get("/{sheetid}")]
async fn get_sheetid(
    data: web::Data<crate::AppData>,
    sheetid: Option<web::Path<SheetId>>,
) -> impl Responder {
    let Some(sheetid) = sheetid else {
        return web::Json(GetSheetIdResponse::Failure {
            error: "invalid sheetid".into(),
        })
        .customize()
        .with_status(StatusCode::BAD_REQUEST);
    };

    match data.db.get_sheet(&sheetid, data.no_lookup_nulls).await {
        Ok(content) => web::Json(GetSheetIdResponse::Success(content)).customize(),
        Err(why) => web::Json(GetSheetIdResponse::Failure {
            error: why.to_string(),
        })
        .customize()
        .with_status(StatusCode::BAD_REQUEST),
    }
}

#[cfg(test)]
mod tests;
