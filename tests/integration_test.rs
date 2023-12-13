use std::{process::Stdio, time::Duration};

use reqwest::header::HeaderMap;
use std::process::{Child, Command};
use tokio::net::TcpStream;

use serde_json::Value as JsonValue;

fn get_url(part: impl std::fmt::Display) -> String {
    format!("http://localhost:8080{part}")
}

async fn set_cell(
    client: &reqwest::Client,
    sheet_id: &str,
    column: &str,
    row: i64,
    value: JsonValue,
) -> bool {
    client
        .post(get_url(format_args!("/sheet/{sheet_id}")))
        .body(format!(
            r#"{{ "column": "{column}", "row": {row}, "value": {} }}"#,
            serde_json::to_string(&value).unwrap()
        ))
        .send()
        .await
        .unwrap()
        .status()
        .is_success()
}

struct KillOnDrop(pub Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

#[tokio::test]
async fn using_a_client() {
    let handle = KillOnDrop(
        Command::new("cargo")
            .arg("run")
            .stdout(Stdio::null())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .env("RUST_LOG", "trace")
            .env("MEMORY_DB", "1")
            .spawn()
            .expect("server failed to spawn"),
    );

    // wait for the server to start up (10 seconds max)
    for _ in 0..100 {
        if TcpStream::connect("localhost:8080").await.is_ok() {
            break;
        } else {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse().unwrap());
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    let resp = client
        .post(get_url("/sheet"))
        .body(
            r#"{
        "columns": [
            {"name": "A", "type": "string"},
            {"name": "B", "type": "boolean"},
            {"name": "C", "type": "string"}
        ]}"#,
        )
        .send()
        .await
        .unwrap();
    dbg!(&resp);
    assert!(resp.status().is_success());
    let sheet_id = resp
        .json::<JsonValue>()
        .await
        .unwrap()
        .get("sheet_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let sheet_id = &*sheet_id;

    assert!(set_cell(&client, sheet_id, "A", 1, JsonValue::String("hello!".into())).await);
    assert!(set_cell(&client, sheet_id, "A", 1, JsonValue::String("hello!!!!".into())).await); // should overwrite the previous one
    assert!(set_cell(&client, sheet_id, "A", 2, JsonValue::String("goodbye!".into())).await);
    assert!(!set_cell(&client, sheet_id, "A", 1, JsonValue::Number(60.into())).await); // wrong type

    ////////////// end of A values, start of B values
    assert!(set_cell(&client, sheet_id, "B", 0, JsonValue::Bool(true)).await);
    assert!(
        set_cell(&client, sheet_id, "B", 1, JsonValue::String(r#"lookup("B", 0)"#.into())).await
    );
    assert!(
        set_cell(&client, sheet_id, "B", 2, JsonValue::String(r#"lookup("B", 1)"#.into())).await
    );
    assert!(
        !set_cell(&client, sheet_id, "B", 2, JsonValue::String(r#"blahlookup("B", 50)"#.into()))
            .await // wrong type
    );

    // this creates a cycle - we check to see that it fails
    assert!(
        !set_cell(&client, sheet_id, "B", 0, JsonValue::String(r#"lookup("B", 2)"#.into())).await
    );

    ////////////// end of B values, start of C values
    assert!(
        set_cell(&client, sheet_id, "C", 1, JsonValue::String(r#"lookup("A", 1)"#.into())).await
    ); // cross-column lookup
    assert!(
        set_cell(&client, sheet_id, "C", 2, JsonValue::String(r#"lookup("A", 2)"#.into())).await
    ); // cross-column lookup

    //////////////

    let resp = client
        .get(get_url(format_args!("/sheet/{sheet_id}")))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let mut json: JsonValue = resp.json().await.unwrap();
    let columns = json.get_mut("columns").unwrap();

    columns
        .as_object_mut()
        .unwrap()
        .iter_mut()
        .for_each(|(_, column)| {
            column
                .as_array_mut()
                .unwrap()
                .sort_unstable_by_key(|x| x.get("row").unwrap().as_i64().unwrap())
        });

    assert_eq!(
        columns,
        &serde_json::json! ({
            "A": [
                {
                    "row": 1,
                    "value": "hello!!!!"
                },
                {
                    "row": 2,
                    "value": "goodbye!"
                }
            ],
            "B": [
                {
                    "row": 0,
                    "value": true
                },
                {
                    "row": 1,
                    "value": true
                },
                {
                    "row": 2,
                    "value": true
                }
            ],
            "C": [
                {
                    "row": 1,
                    "value": "hello!!!!"
                },
                {
                    "row": 2,
                    "value": "goodbye!"
                }
            ]
        })
    );

    drop(handle);
}
