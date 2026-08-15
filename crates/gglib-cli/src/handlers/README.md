# handlers

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-complexity.json)

<!-- module-docs:start -->

CLI command handlers implementing the business logic for each command.

## Purpose

This module contains the **handler functions** that implement the actual logic for CLI commands. Handlers are called by the command parser after arguments are validated.

## Architecture Pattern

**Separation of Concerns**

```text
┌─────────────────────────────────────────────────────────────┐
│                      CLI Flow                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  User Input → Parser → Handler → Service → Port → Adapter  │
│     (clap)   (parser.rs) (this)  (core)   (core)  (infra)  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Handlers** sit between the CLI parser and the service layer:
- Extract validated arguments from parser
- Format inputs for service calls
- Handle errors and format output
- Present results to user

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`benchmark.rs`](benchmark.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-benchmark-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-benchmark-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-benchmark-coverage.json) |
| [`completions.rs`](completions.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-completions-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-completions-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-completions-coverage.json) |
| [`gui.rs`](gui.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-gui-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-gui-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-gui-coverage.json) |
| [`history.rs`](history.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-history-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-history-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-history-coverage.json) |
| [`mcp_cli.rs`](mcp_cli.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-mcp_cli-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-mcp_cli-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-mcp_cli-coverage.json) |
| [`proxy_cache_clear.rs`](proxy_cache_clear.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-proxy_cache_clear-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-proxy_cache_clear-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-proxy_cache_clear-coverage.json) |
| [`web.rs`](web.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-web-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-web-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-web-coverage.json) |
| [`agent_chat/`](agent_chat/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-coverage.json) |
| [`config/`](config/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config-coverage.json) |
| [`daemon/`](daemon/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-daemon-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-daemon-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-daemon-coverage.json) |
| [`inference/`](inference/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-inference-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-inference-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-inference-coverage.json) |
| [`model/`](model/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-model-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-model-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-model-coverage.json) |
| [`proxy_dashboard/`](proxy_dashboard/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-proxy_dashboard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-proxy_dashboard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-proxy_dashboard-coverage.json) |
| [`up/`](up/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-coverage.json) |
<!-- module-table:end -->

## Handler Organization

Handlers are grouped by the command they serve, one directory per family. The
list below is the directory tree, not a summary of it — an earlier version of
this section described `add.rs`, `list.rs`, `download/{start,pause,resume}.rs`
and `question.rs` at this level, none of which have been here since the
handlers were grouped.

| Path | Serves |
|------|--------|
| `model/` | `gglib model …` — add, list, inspect, explain, remove, capabilities, and `download/` |
| `inference/` | `serve`, `proxy`, `chat`, and `agent_question` (`gglib q`) |
| `config/` | `gglib config …` — `settings/`, `paths.rs`, `llama*.rs`, `check_deps/`, `fast_downloads.rs` |
| `agent_chat/` | The interactive agent REPL behind `gglib chat` |
| `daemon/` | `gglib daemon run / status / stop` |
| `up/` | `gglib up` — the one-command setup path |
| `proxy_dashboard/` | `gglib proxy dashboard` — the live terminal view |
| `benchmark.rs` | `gglib benchmark …`, including `tune` |
| `mcp_cli.rs` | `gglib mcp …` |
| `history.rs`, `web.rs`, `gui.rs`, `completions.rs`, `proxy_cache_clear.rs` | One command each |

Each directory carries its own README with the detail; this table exists so a
newcomer can find the right one, and so it stays true by being short.


## Handler Pattern

### Standard Handler Structure
```rust,ignore
pub async fn handle_command(
    args: CommandArgs,
    services: &AppServices,
) -> Result<(), HandlerError> {
    // 1. Extract and validate arguments
    let id = &args.model_id;
    
    // 2. Call service layer
    let model = services.model_service.get_model(id).await?;
    
    // 3. Format output
    println!("Model: {}", model.name);
    
    // 4. Return result
    Ok(())
}
```

### Error Handling
Handlers convert service errors to CLI-friendly messages:
```rust,ignore
pub enum HandlerError {
    NotFound(String),
    InvalidInput(String),
    ServiceError(String),
}

impl From<ServiceError> for HandlerError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::NotFound(id) => 
                HandlerError::NotFound(format!("Model '{}' not found", id)),
            _ => HandlerError::ServiceError(err.to_string()),
        }
    }
}
```

### User Interaction
Handlers use `../utils/input.rs` for prompts:
```rust,ignore
use crate::utils::input;

if !args.force {
    let confirm = input::confirm("Delete model?")?;
    if !confirm {
        return Ok(());
    }
}
```

## Output Formatting

### Tables
Use `../presentation/tables` for structured output:
```rust,ignore
use crate::presentation::tables::ModelTable;

let models = service.list_models().await?;
ModelTable::new(models).print();
```

### Progress
Use progress bars for long operations:
```rust,ignore
use indicatif::ProgressBar;

let pb = ProgressBar::new(total_bytes);
pb.set_style(/* ... */);
// Update in download callback
```

## Dependencies

Handlers depend on:
- **Service layer**: `gglib-core::services::*` for business logic
- **Domain types**: `gglib-core::domain::*` for entities
- **Presentation**: `../presentation/` for formatting
- **Utils**: `../utils/` for input/output helpers
- **Error types**: `../error.rs` for CLI error handling

## Testing

Handler tests focus on:
- Argument parsing edge cases
- Service call correctness
- Error message formatting
- Output validation

Use mock services for unit tests:
```rust
#[tokio::test]
async fn test_add_handler() {
    let mut mock_service = MockModelService::new();
    mock_service.expect_add_model()
        .returning(|_| Ok(()));
    
    let result = handlers::add::handle_add(
        args,
        &mock_service,
    ).await;
    
    assert!(result.is_ok());
}
```

<!-- module-docs:end -->
