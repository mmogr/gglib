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
| `remote/` | `gglib remote enable / disable / status / invite / list / forget / join / disconnect / key` — the tunnel that puts one machine's proxy on another (ADR 0012) |
| `benchmark.rs`, `benchmark_verdicts.rs` | `gglib benchmark …`, including `tune`; the second holds the agentic report's three verdict blocks |
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
