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
| [`explain_display_tests.rs`](explain_display_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-explain_display_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-explain_display_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-explain_display_tests-coverage.json) |
| [`explain_display.rs`](explain_display.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-explain_display-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-explain_display-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-explain_display-coverage.json) |
| [`inspect_display.rs`](inspect_display.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-inspect_display-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-inspect_display-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-inspect_display-coverage.json) |
| [`model_display.rs`](model_display.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-model_display-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-model_display-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-model_display-coverage.json) |
| [`style.rs`](style.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-style-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-style-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-style-coverage.json) |
| [`tables.rs`](tables.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tables-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tables-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-tables-coverage.json) |
<!-- module-table:end -->
## Components

### Model Display
**Module:** `model_display.rs`

Formats model information for terminal output.

**Key Functions:**
- `display_model_summary()` - Renders model details with metadata
- `ModelSummaryOpts` - Which fields to include, built by `with_title()` or `for_removal()`

**Example:**
```rust,ignore
use crate::presentation::model_display::{display_model_summary, ModelSummaryOpts};

let model = /* ... */;
display_model_summary(&model, ModelSummaryOpts::with_title("Model created:"));
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
- `format_relative_time(datetime_str: &str)` - Renders a SQLite timestamp as "5 min ago"
- `truncate_string(s: &str, max_len: usize)` - Safely truncates with ellipsis
- `print_separator(width: usize)` - Prints a horizontal separator

**Example:**
```rust,ignore
use crate::presentation::tables::{print_separator, truncate_string};

// Column-safe truncation
println!("Name: {}", truncate_string("a-very-long-model-name.gguf", 12));

// Separators
print_separator(80);
// ================================================================================
```

### Table Formatting Pattern

Most commands use a consistent table pattern:

```rust,ignore
// Header
println!("{:<15} {:<20} {:<10}", "ID", "Name", "Added");
print_separator(45);

// Rows
for model in models {
    println!(
        "{:<15} {:<20} {:<10}",
        model.id,
        truncate_string(&model.name, 20),
        format_relative_time(&model.added_at)
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
fn relative_time_bad_parse_returns_raw() {
    assert_eq!(format_relative_time("not-a-date"), "not-a-date");
}

#[test]
fn truncate_long_string_gets_ellipsis() {
    // max_len=5: 4 chars of content + ellipsis = 5 chars total
    assert_eq!(truncate_string("hello world", 5), "hell\u{2026}");
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
