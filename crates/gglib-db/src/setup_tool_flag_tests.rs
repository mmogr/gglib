//! Step 10: which rows take their stored tool answer, and which are left
//! exactly as they were.

use gglib_core::domain::ModelCapabilities;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

use super::super::create_schema;
use super::super::migrations::{LATEST, run, user_version};

const TOOLS: i64 = ModelCapabilities::SUPPORTS_TOOL_CALLS.bits() as i64;
const SYSTEM: i64 = ModelCapabilities::SUPPORTS_SYSTEM_ROLE.bits() as i64;
const REASONING: i64 = ModelCapabilities::SUPPORTS_REASONING.bits() as i64;

/// One connection, so every statement and pragma reads the one database.
async fn library() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    create_schema(&pool).await.unwrap();
    pool
}

/// Add a model named `name` holding `capabilities` and `caps` as stored text.
async fn plant(pool: &SqlitePool, name: &str, capabilities: Option<i64>, caps: Option<&str>) {
    sqlx::query(
        "INSERT INTO models (name, file_path, added_at, model_key, capabilities, template_caps) \
         VALUES (?, ?, 'then', ?, ?, ?)",
    )
    .bind(name)
    .bind(format!("/models/{name}.gguf"))
    .bind(format!("local:{name}"))
    .bind(capabilities)
    .bind(caps)
    .execute(pool)
    .await
    .unwrap();
}

/// The `capabilities` stored for `name`.
async fn bits_of(pool: &SqlitePool, name: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT capabilities FROM models WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Stamp the library as a build at step 9 left it.
async fn stamp_older(pool: &SqlitePool) {
    sqlx::query("PRAGMA user_version = 9")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn an_answered_row_takes_its_answer_and_keeps_its_other_bits() {
    let pool = library().await;
    let yes = Some(r#"{"supports_tools":false,"supports_tool_calls":true}"#);
    let no = Some(r#"{"supports_tool_calls":false,"supports_tools":true}"#);
    plant(&pool, "yes", Some(SYSTEM), yes).await;
    plant(&pool, "no", Some(SYSTEM | TOOLS | REASONING), no).await;
    plant(&pool, "null bits", None, yes).await;
    stamp_older(&pool).await;

    run(&pool, LATEST).await.unwrap();

    assert_eq!(bits_of(&pool, "yes").await, Some(SYSTEM | TOOLS));
    assert_eq!(bits_of(&pool, "no").await, Some(SYSTEM | REASONING));
    assert_eq!(bits_of(&pool, "null bits").await, Some(TOOLS));
    assert_eq!(user_version(&pool).await.unwrap(), LATEST);
}

/// No caps, caps that did not say, a value that is not a boolean, text that
/// is not JSON, and a row whose bit already agrees keep their bits.
#[tokio::test]
async fn a_row_without_an_answer_is_left_alone() {
    let pool = library().await;
    let kept = [
        ("never launched", Some(TOOLS | SYSTEM), None),
        (
            "unsaid",
            Some(REASONING),
            Some(r#"{"supports_tool_calls":null}"#),
        ),
        ("absent", Some(TOOLS), Some(r#"{"supports_tools":false}"#)),
        (
            "string",
            Some(TOOLS),
            Some(r#"{"supports_tool_calls":"no"}"#),
        ),
        ("number", Some(SYSTEM), Some(r#"{"supports_tool_calls":1}"#)),
        ("garbage", Some(SYSTEM), Some("not json")),
        (
            "array",
            Some(TOOLS),
            Some(r#"[{"supports_tool_calls":false}]"#),
        ),
        (
            "agrees",
            Some(TOOLS),
            Some(r#"{"supports_tool_calls":true}"#),
        ),
        ("null bits", None, Some(r#"{"supports_tool_calls":false}"#)),
    ];
    for (name, bits, caps) in kept {
        plant(&pool, name, bits, caps).await;
    }
    stamp_older(&pool).await;

    run(&pool, LATEST).await.unwrap();

    for (name, bits, _) in kept {
        assert_eq!(bits_of(&pool, name).await, bits, "{name}");
    }
}
