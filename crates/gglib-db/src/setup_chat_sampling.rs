//! Step 9: a chat's settings stop holding four sampling keys.
//!
//! A `#[path]` child of `setup.rs`. `gglib chat` and `gglib q` used to save
//! `--temperature`, `--top-p`, `--top-k` and `--repeat-penalty` in the
//! chat's `settings`, and each turn sent them as a client's sampling. The
//! flags are gone and nothing reads the keys; a write keeps a key it does not
//! know, so each one goes from every chat's settings here, and every other
//! key keeps its value and its place.
//!
//! `settings` is read leniently, so a row whose text is not a JSON object is
//! left as it is rather than failing the open; the `CASE` makes sure
//! `json_extract` never reads one. A row with none of the four is not
//! rewritten.

use anyhow::Result;
use sqlx::SqliteConnection;

/// Every chat's `settings`, the four sampling keys taken out.
const STRIP: &str = "UPDATE chat_conversations
    SET settings = json_remove(settings,
        '$.temperature', '$.top_p', '$.top_k', '$.repeat_penalty')
    WHERE CASE WHEN json_valid(settings) AND json_type(settings) = 'object'
        THEN json_type(settings, '$.temperature') IS NOT NULL
            OR json_type(settings, '$.top_p') IS NOT NULL
            OR json_type(settings, '$.top_k') IS NOT NULL
            OR json_type(settings, '$.repeat_penalty') IS NOT NULL
        ELSE 0 END";

/// Step 9: strip the four sampling keys. See the module docs.
pub(super) async fn strip_chat_sampling(conn: &mut SqliteConnection) -> Result<()> {
    let stripped = sqlx::query(STRIP)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if stripped > 0 {
        tracing::info!(stripped, "sampling keys removed from chat settings");
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_chat_sampling_tests.rs"]
mod setup_chat_sampling_tests;
