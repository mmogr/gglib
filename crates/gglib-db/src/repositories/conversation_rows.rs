//! A conversation's row changed as `ChatHistoryRepository::update_conversation`
//! says: its settings key by key, so a key this build does not know or cannot
//! read stays as it is stored.

use sqlx::{Row, SqlitePool};

use gglib_core::domain::chat::ConversationUpdate;
use gglib_core::ports::chat_history::ChatHistoryError;

fn db(error: &sqlx::Error) -> ChatHistoryError {
    ChatHistoryError::Database(error.to_string())
}

/// Make `update` to conversation `id`, in one transaction that reads the row
/// it writes; nothing at all for an update that names nothing. The settings'
/// edit is made on the stored text by `json_set` and `json_remove`, which
/// keep every other key and its text; it is refused for text that is not a
/// JSON object, and nothing is written.
pub(super) async fn update(
    pool: &SqlitePool,
    id: i64,
    update: ConversationUpdate,
) -> Result<(), ChatHistoryError> {
    if update.title.is_none()
        && update.system_prompt.is_none()
        && update.settings.is_empty()
        && update.model_id.is_none()
    {
        return Ok(());
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|e| db(&e))?;
    let row = sqlx::query(
        "SELECT title, model_id, system_prompt, settings IS NULL AS unset, \
           CASE WHEN json_valid(settings) THEN json_type(settings) = 'object' END AS object \
         FROM chat_conversations WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| db(&e))?
    .ok_or(ChatHistoryError::ConversationNotFound(id))?;
    let unset: bool = row.get("unset");
    let object = row.get::<Option<bool>, _>("object").unwrap_or(false);
    let edit = update.settings;
    if !edit.is_empty() && !unset && !object {
        return Err(ChatHistoryError::UnreadableSettings(id));
    }

    // No edit leaves the text as it is, and removing keys from settings
    // never stored leaves them never stored.
    let sets = edit.values().any(Option::is_some);
    let mut settings = String::from("COALESCE(settings, '{}')");
    let mut binds = Vec::new();
    for (key, value) in &edit {
        let path = format!("$.\"{key}\"");
        settings = if let Some(value) = value {
            binds.extend([path, value.to_string()]);
            format!("json_set({settings}, ?, json(?))")
        } else {
            binds.push(path);
            format!("json_remove({settings}, ?)")
        };
    }
    if edit.is_empty() || (unset && !sets) {
        settings = String::from("settings");
        binds.clear();
    }

    let title: String = update.title.unwrap_or_else(|| row.get("title"));
    let model_id = update.model_id.unwrap_or_else(|| row.get("model_id"));
    let prompt = update
        .system_prompt
        .unwrap_or_else(|| row.get("system_prompt"));
    let sql = format!(
        "UPDATE chat_conversations SET title = ?, model_id = ?, system_prompt = ?, \
         settings = {settings}, updated_at = datetime('now') WHERE id = ?"
    );
    let mut query = sqlx::query(&sql).bind(title).bind(model_id).bind(prompt);
    for bind in binds {
        query = query.bind(bind);
    }
    query.bind(id).execute(&mut *tx).await.map_err(|e| db(&e))?;
    tx.commit().await.map_err(|e| db(&e))
}
