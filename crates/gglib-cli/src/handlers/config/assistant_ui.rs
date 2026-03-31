//! assistant-ui management command handler.
//!
//! Thin dispatcher that routes `AssistantUiCommand` variants to the appropriate
//! functions in `gglib_runtime::assistant_ui`. Contains no business logic.

use anyhow::Result;

use crate::assistant_ui_commands::AssistantUiCommand;

/// Dispatch an `assistant-ui` sub-command to the appropriate `gglib_runtime` handler.
pub fn dispatch(command: AssistantUiCommand) -> Result<()> {
    use gglib_runtime::assistant_ui::{
        handle_install as handle_assistant_install, handle_status as handle_assistant_status,
        handle_update as handle_assistant_update,
    };

    match command {
        AssistantUiCommand::Install => {
            handle_assistant_install().map_err(|e| anyhow::anyhow!(e))?;
        }
        AssistantUiCommand::Update => {
            handle_assistant_update().map_err(|e| anyhow::anyhow!(e))?;
        }
        AssistantUiCommand::Status => {
            handle_assistant_status().map_err(|e| anyhow::anyhow!(e))?;
        }
    }
    Ok(())
}
