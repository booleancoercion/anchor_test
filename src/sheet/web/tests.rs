use actix_web::http::header::ContentType;
use actix_web::test;

use crate::sheet::tests::VALID_POST_PAYLOAD;

use super::PostResponse;

// this is a macro because frankly writing the return type would be a hassle
macro_rules! init_service {
    () => {{
        let _ = ::env_logger::builder()
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
        ::std::dbg!(&body);
        let json: ::serde_json::Value = ::serde_json::from_slice(&body).unwrap();
        ::std::assert!(json.is_object());
        ::std::assert_eq!(
            json.as_object()
                .unwrap()
                .keys()
                .collect::<::std::vec::Vec<_>>(),
            ["error"]
        );
        ::std::assert!(json["error"].is_string());
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
        .set_payload(
            r#"{"columns": [{"name": "A", "type": "string"}, {"name": "A", "type": "boolean"}]}"#,
        )
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

async fn get_standard_sheet<S, B>(app: &S) -> anyhow::Result<String>
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse<B>,
        Error = actix_web::Error,
    >,
    B: actix_web::body::MessageBody,
{
    let req = test::TestRequest::post()
        .uri("/sheet")
        .set_payload(VALID_POST_PAYLOAD)
        .insert_header(ContentType::json())
        .to_request();

    let resp: PostResponse = test::call_and_read_body_json(&app, req).await;
    match resp {
        PostResponse::Success { sheet_id } => Ok(sheet_id),
        PostResponse::Failure { error } => anyhow::bail!("Error: {error:#?}"),
    }
}

#[actix_web::test]
async fn test_post_sheetid_simple() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_post_sheetid_update_cell() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": 43 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_post_sheetid_invalid_type() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "A", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_invalid_column() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "abracadabra", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_invalid_sheet() {
    let app = init_service!();

    let req = test::TestRequest::post()
        .uri("/sheet/abCDefGHijklMnOPqrst1234")
        .set_payload(r#"{ "column": "abracadabra", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_lookup_simple() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_post_sheetid_lookup_self_referential() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 5)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_lookup_wrong_type() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"A\", 5)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_same_type_different_col() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B2\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_post_sheetid_lookup_chain() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 4, "value": "lookup(\"B\", 3)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_post_sheetid_lookup_cycle2() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 4, "value": "lookup(\"B\", 5)" }"#)
        .insert_header(ContentType::json())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_lookup_cycle3() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 4, "value": "lookup(\"B\", 3)" }"#)
        .insert_header(ContentType::json())
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 3, "value": "lookup(\"B\", 5)" }"#)
        .insert_header(ContentType::json())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_is_error_response!(resp);
}

#[actix_web::test]
async fn test_post_sheetid_update_cell_lookup() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_post_sheetid_update_lookup_cell() {
    let app = init_service!();

    let sheet_id = get_standard_sheet(&app).await.expect("valid sheet failed");
    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": "lookup(\"B\", 4)" }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());

    let req = test::TestRequest::post()
        .uri(&format!("/sheet/{sheet_id}"))
        .set_payload(r#"{ "column": "B", "row": 5, "value": 42 }"#)
        .insert_header(ContentType::json())
        .to_request();

    let resp = test::call_service(&app, req).await;
    dbg!(&resp);
    assert!(resp.status().is_success());
}
