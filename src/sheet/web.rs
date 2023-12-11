use actix_web::{http::StatusCode, post, web, Responder};
use serde::Serialize;

use crate::db::SheetId;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(post);
}

#[derive(Serialize)]
#[serde(untagged)]
enum PostResponse {
    Success { sheet_id: String },

    Failure { error: String },
}

#[derive(Serialize)]
#[serde(untagged)]
enum PostSheetIdResponse {
    Success {},

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

#[cfg(test)]
mod tests {
    use actix_web::http::header::ContentType;
    use actix_web::test;

    use crate::sheet::tests::VALID_POST_PAYLOAD;

    // this is a macro because frankly writing the return type would be a hassle
    macro_rules! init_service {
        () => {{
            let _ = env_logger::builder()
                .is_test(true)
                .filter_level(::log::LevelFilter::max())
                .try_init();
            let db = crate::db::Db::new_memory().await.unwrap();
            let data = ::actix_web::web::Data::new(crate::AppData { db });
            ::actix_web::test::init_service(
                ::actix_web::App::new()
                    .app_data(data)
                    .wrap(::actix_web::middleware::NormalizePath::trim())
                    .service(::actix_web::web::scope("/sheet").configure(super::config)),
            )
            .await
        }};
    }

    #[actix_web::test]
    async fn test_post_success_simple() {
        let app = init_service!();

        let req = test::TestRequest::post()
            .uri("/sheet")
            .set_payload(VALID_POST_PAYLOAD)
            .insert_header(ContentType::json())
            .to_request();

        let resp = test::call_service(&app, req).await;
        dbg!(&resp);
        dbg!(resp.response().body());
        assert!(resp.status().is_success())
    }

    #[actix_web::test]
    async fn test_post_success() {
        let app = init_service!();

        let req = test::TestRequest::post()
            .uri("/sheet")
            .set_payload(VALID_POST_PAYLOAD)
            .insert_header(ContentType::json())
            .to_request();

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert!(resp.is_object());
        assert_eq!(
            resp.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["sheet_id"]
        )
    }

    macro_rules! assert_is_error_response {
        ($resp:expr) => {{
            ::std::assert!($resp.status().is_client_error());

            let body = ::actix_web::body::to_bytes($resp.into_body())
                .await
                .unwrap();
            dbg!(&body);
            let json: ::serde_json::Value = ::serde_json::from_slice(&body).unwrap();
            assert!(json.is_object());
            assert_eq!(
                json.as_object()
                    .unwrap()
                    .keys()
                    .collect::<::std::vec::Vec<_>>(),
                ["error"]
            )
        }};
    }

    #[actix_web::test]
    async fn test_post_no_payload() {
        let app = init_service!();

        let req = test::TestRequest::post().uri("/sheet").to_request();

        let resp = test::call_service(&app, req).await;
        assert_is_error_response!(resp);
    }

    #[actix_web::test]
    async fn test_post_invalid_json() {
        let app = init_service!();

        let req = test::TestRequest::post()
            .uri("/sheet")
            .set_payload("{{}{}}{}{{{{'yoohoo!!!dikjnmqwiodnw")
            .insert_header(ContentType::json())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_is_error_response!(resp);
    }

    #[actix_web::test]
    async fn test_post_valid_json_invalid_format() {
        let app = init_service!();

        let req = test::TestRequest::post()
            .uri("/sheet")
            .set_payload(r#"{"this is": "technically valid json"}"#)
            .insert_header(ContentType::json())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_is_error_response!(resp);
    }

    #[actix_web::test]
    async fn test_post_schema_with_duplicates() {
        let app = init_service!();

        let req = test::TestRequest::post()
            .uri("/sheet")
            .set_payload(r#"{"columns": [{"name": "A", "type": "string"}, {"name": "A", "type": "boolean"}]}"#)
            .insert_header(ContentType::json())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_is_error_response!(resp);
    }
}
