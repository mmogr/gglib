//! Step 10: each launched model's tool flag is llama-server's stored answer.
//!
//! A `#[path]` child of `setup.rs`. Every launch stores llama-server's
//! `chat_template_caps` on the model row (`template_caps`, ADR 0007), and
//! from this build a launch writes its `supports_tool_calls` into the tool
//! bit of `capabilities` when the answer is new or changed. A row an older
//! build launched already holds its answer, so no launch will write it; this
//! step does, once. A row with no answer, or text that is not JSON, is left
//! as it is, and so are the other bits. A `NULL` `capabilities` counts as no
//! bits. A row whose bit already agrees is not rewritten.

use anyhow::Result;
use gglib_core::domain::ModelCapabilities;
use sqlx::SqliteConnection;

/// Each answered row's tool bit (`?1`), set to its stored answer.
const SET: &str = "UPDATE models
    SET capabilities = IIF(json_extract(template_caps, '$.supports_tool_calls'),
        COALESCE(capabilities, 0) | ?1, COALESCE(capabilities, 0) & ~?1)
    WHERE CASE WHEN json_valid(template_caps)
        THEN CASE WHEN json_type(template_caps, '$.supports_tool_calls') IN ('true', 'false')
            THEN (COALESCE(capabilities, 0) & ?1 != 0)
                != json_extract(template_caps, '$.supports_tool_calls')
            ELSE 0 END
        ELSE 0 END";

/// Step 10: set the tool flag to the stored answer. See the module docs.
pub(super) async fn take_the_stored_tool_answer(conn: &mut SqliteConnection) -> Result<()> {
    let bit = i64::from(ModelCapabilities::SUPPORTS_TOOL_CALLS.bits());
    let set = sqlx::query(SET)
        .bind(bit)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if set > 0 {
        tracing::info!(set, "tool flags set to llama-server's stored answer");
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_tool_flag_tests.rs"]
mod setup_tool_flag_tests;
