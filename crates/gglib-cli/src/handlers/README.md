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
| [`add.rs`](add) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-add-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-add-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-add-coverage.json) |
| [`chat.rs`](chat) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-chat-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-chat-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-chat-coverage.json) |
| [`config.rs`](config) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-config-coverage.json) |
| [`list.rs`](list) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-list-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-list-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-list-coverage.json) |
| [`paths.rs`](paths) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-paths-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-paths-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-paths-coverage.json) |
| [`remove.rs`](remove) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-remove-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-remove-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-remove-coverage.json) |
| [`serve.rs`](serve) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-serve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-serve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-serve-coverage.json) |
| [`update.rs`](update) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-update-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-update-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-update-coverage.json) |
| [`check_deps/`](check_deps/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-coverage.json) |
| [`download/`](download/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-coverage.json) |
<!-- module-table:end -->

## Handler Organization

### Model Management
- **`add.rs`** - Add new models to catalog
  - Parse model paths or HuggingFace IDs
  - Validate model files
  - Call `ModelService::add_model`
  - Display confirmation

- **`list.rs`** - List all models
  - Call `ModelService::list_models`
  - Format as table using `../presentation/tables`
  - Filter and sort options

- **`remove.rs`** - Remove models from catalog
  - Confirm deletion
  - Call `ModelService::remove_model`
  - Handle errors if model in use

- **`serve.rs`** - Start/stop model servers
  - Validate model exists
  - Call `ServerService::start`/`stop`
  - Display server URL and status

- **`update.rs`** - Update model metadata
  - Parse update parameters
  - Call `ModelService::update_model`
  - Display changes

### Download Management
- **`download/`** - Download command handlers
  - `start.rs` - Start new download
  - `list.rs` - List active downloads
  - `pause.rs` - Pause download
  - `resume.rs` - Resume paused download
  - `cancel.rs` - Cancel download

### Configuration
- **`config.rs`** - Configuration management
  - Get/set configuration values
  - Validate configuration
  - Display current settings

- **`paths.rs`** - Display path information
  - Show data directories
  - Show model paths
  - Show log locations

### System
- **`check_deps/`** - Dependency checking
  - `check.rs` - Check system dependencies
  - `install.rs` - Install missing dependencies

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
