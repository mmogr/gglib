# presentation

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-complexity.json)

<!-- module-docs:start -->

CLI presentation layer providing formatting and display utilities.

## Purpose

This module contains reusable presentation logic for formatting CLI output. It ensures consistent, user-friendly display of data across all commands without mixing presentation concerns with business logic.

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                  presentation Module                         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Handler → Formatter → Terminal Output                      │
│            (this)                                            │
│                                                              │
│  Domain Objects → Display Objects → Formatted Strings       │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

## Design Principles

1. **Format-Only** - No domain transforms, no business logic
2. **Reusability** - Shared across all CLI commands
3. **Consistency** - Uniform look and feel
4. **Separation** - Presentation decoupled from data layer
## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`model_display.rs`](model_display) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-model_display-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-model_display-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-model_display-coverage.json) |
| [`tables.rs`](tables) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tables-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tables-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tables-coverage.json) |
<!-- module-table:end -->
## Components

### Model Display
**Module:** `model_display.rs`

Formats model information for terminal output.

**Key Functions:**
- `display_model_summary()` - Renders model details with metadata
- `DisplayStyle` enum - Controls verbosity (compact, detailed)
- `ModelSummaryOpts` - Configuration for display options

**Example:**
```rust,ignore
use gglib_cli::presentation::model_display::{display_model_summary, DisplayStyle};

let model = /* ... */;
display_model_summary(&model, DisplayStyle::Detailed)?;
```

**Output:**
```text
Model: Llama 2 7B Chat
ID: 1
Path: /Users/user/models/llama-2-7b-chat.Q4_K_M.gguf
Architecture: llama
Parameters: 7B
Quantization: Q4_K_M
Context Length: 4096
```

### Tables
**Module:** `tables.rs`

Provides table formatting utilities and helper functions.

**Key Functions:**
- `format_optional(value: Option<T>)` - Formats optional values as "N/A" or actual value
- `truncate_string(s: &str, max_len: usize)` - Safely truncates with ellipsis
- `print_separator(char: char, width: usize)` - Prints horizontal separators

**Example:**
```rust,ignore
use gglib_cli::presentation::tables::{format_optional, print_separator};

// Optional value formatting
let name = Some("model.gguf");
println!("Name: {}", format_optional(name)); // "Name: model.gguf"

let missing = None::<String>;
println!("Tags: {}", format_optional(missing)); // "Tags: N/A"

// Separators
print_separator('=', 80);
// ================================================================================
```

### Table Formatting Pattern

Most commands use a consistent table pattern:

```rust,ignore
// Header
println!("{:<15} {:<20} {:<10}", "ID", "Name", "Size");
print_separator('-', 45);

// Rows
for model in models {
    println!(
        "{:<15} {:<20} {:<10}",
        model.id,
        truncate_string(&model.name, 20),
        format_optional(model.size_gb)
    );
}
```

## Usage Guidelines

### When to Use This Module
- Formatting domain objects for display
- Creating consistent table layouts
- Handling optional/missing data display
- Truncating long strings for terminal width

### When NOT to Use This Module
- Domain transformations (belongs in core/services)
- Data validation (belongs in core/ports)
- Business logic (belongs in core/services)
- Database queries (belongs in repositories)

### View-Model Pattern

For complex displays, create CLI-specific view models in handlers:

```rust,ignore
// Handler creates view model
pub struct ModelListView {
    pub id: i64,
    pub display_name: String,
    pub status: String,
}

impl From<Model> for ModelListView {
    fn from(model: Model) -> Self {
        Self {
            id: model.id,
            display_name: format!("{} ({})", model.name, model.quantization),
            status: if model.is_available { "Ready" } else { "Downloading" },
        }
    }
}

// Then use presentation module for formatting
display_model_list(&view_models);
```

## Dependencies

- **Standard library only** - No external presentation frameworks
- Uses ANSI color codes for terminal coloring
- Relies on fixed-width formatting (`println!` with format strings)

## Testing

Tests focus on:
- Correct formatting of edge cases (empty, None, very long strings)
- Truncation behavior
- Table alignment
- Color code application

```rust,ignore
#[test]
fn test_format_optional() {
    assert_eq!(format_optional(Some(42)), "42");
    assert_eq!(format_optional(None::<i32>), "N/A");
}

#[test]
fn test_truncate_string() {
    let long = "This is a very long string";
    assert_eq!(truncate_string(long, 10), "This is...");
}
```

## Design Notes

1. **Keep It Simple** - Plain text formatting, no fancy TUI frameworks
2. **Terminal-Friendly** - Assumes standard terminal width (~80-120 chars)
3. **Composable** - Small, focused functions that work together
4. **Testable** - Pure functions with no side effects (except printing)

## Future Considerations

- JSON output mode for scripting (`--json` flag)
- Color theme customization
- Terminal width detection and adaptive layout
- Progress bar utilities (currently in individual handlers)

<!-- module-docs:end -->
