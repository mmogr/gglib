# presentation

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

## Components

### Model Display
**Module:** `model_display.rs`

Formats model information for terminal output.

**Key Functions:**
- `display_model_summary()` - Renders model details with metadata; a model that draws gets no `Parameters` line
- `summary_lines()` - The lines it prints
- `ModelSummaryOpts` - Which fields to include, built by `with_title()` or `for_removal()`

**Example:**
```rust,ignore
use crate::presentation::model_display::{display_model_summary, ModelSummaryOpts};

let model = /* ... */;
display_model_summary(&model, &ModelSummaryOpts::with_title("Model created:"));
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

### Capability Flags
**Module:** `capability_flags.rs`

The one table of capability flags: each flag's name, its bit, and the field of
an override request that sets it. `gglib model capabilities` takes its `--set`
and `--unset` values from the table, and that command and `gglib model inspect`
print from it. The module's tests write the table to
`contracts/models/capability_flags.json` and fail when the file is stale; the
page's test of its own four flags reads that file.

**Key Items:**
- `CAPABILITY_FLAGS` - The four flags, in the order they are listed
- `capability_lines(caps)` - One `name : yes|no` line per flag
- `capability_names()` - The names as the values clap accepts

### Sampling Values
**Module:** `sampling_values.rs`

**Key Functions:**
- `stated_parameters(config: &InferenceConfig)` - Every field the config sets, by name, rendered as it was typed. The serve banner and `gglib model inspect` both list from it.

### Style
**Module:** `style.rs`

The ANSI colour and style constants every CLI module prints with, and the
terminal pieces built on them.

**Key Items:**
- `SUCCESS`, `DANGER`, `WARNING`, `INFO`, `MUTED`, `BOLD`, `DIM`, `RESET` - The escape sequences
- `get_markdown_skin()` - The termimad skin Rich mode renders Markdown with
- `print_info_banner()`, `print_thinking_banner()`, `print_banner_close()` - A banner's top and bottom borders on stderr
- `make_spinner()` - The indicatif spinner shown on stderr while a reply is received

### Explain Display
**Module:** `explain_display.rs`

Draws the sampling half of `gglib model explain`: each resolved sampling
parameter beside the layer that supplied it, from the `SamplingExplanationDto`
inside the explanation the explain call answers with, and under the effort row
the levels the model's template lists when they are known.

**Key Functions:**
- `explanation_text()` - The table as text: heading, one row per parameter, caveats
- `explanation_lines()` - One string per parameter

### Launch Display
**Module:** `launch_display.rs`

Draws the launch half of `gglib model explain`: the command gglib would run,
each argument beside whose choice it was and why, and the context section,
from the `LaunchPlan` the explain call answers with.

**Key Functions:**
- `launch_text()` - The command line (or, in its place, why there is none: the engine is not installed, or the service is older than the line), the argument rows and the context section as text
- `argument_lines()` - One string per argument
- `context_text()` - The context chain and the fit's inputs

### Inspect Display
**Module:** `inspect_display.rs`

Draws `gglib model inspect` from a `ModelDetailDto`; the handler only fetches
the model and branches on `--json`.

**Key Functions:**
- `print_model_detail(dto, show_metadata)` - Every section; `show_metadata` adds the raw GGUF key-value pairs (`--metadata`)

### Tables
**Module:** `tables.rs`

Provides table formatting utilities and helper functions.

**Key Functions:**
- `format_relative_time(datetime_str: &str)` - Renders a `SQLite` timestamp as "5 min ago"
- `truncate_string(s: &str, max_len: usize)` - Safely truncates with ellipsis
- `truncate_with(s: &str, max_len: usize, marker: &str)` - The same cut, ending in `marker`
- `first_chars(s: &str, max: usize)` - The first `max` characters, never part of one
- `short_sha(sha: &str)` - The first 8 characters of a hash
- `format_number(n: u64)` - Renders a count as `500`, `1.5K` or `1.5M`

### Table Layout
**Module:** `table_layout.rs`

The one owner of how wide a printed table and its rules are. A `Table` is
its `Column`s and rows of cells; `Table::lay_out(width)` takes the width as a
value and returns lines no wider than it. In order, it gives way by:
padding first, then shortening what may be shortened (a name at its end, a
path in its middle keeping the file name, prose wrapped onto more lines),
then dropping the lowest-priority column, and for a very narrow width it
prints one `label: value` block per row. With no width (output piped or
redirected) nothing is shortened or dropped. Widths are display widths:
CJK and emoji take two columns, colour codes none.

**Key Items:**
- `Table`, `Column`, `Shorten`, `Align` - A table and how each column gives way
- `Laid` - The laid-out header, ruler and rows; `lines()` and `text()`
- `rule(ch, natural, width)` - A rule between sections, never wider than the terminal
- `wrap(text, width)` - Prose wrapped to a width
- `display_width(text)` - The columns text takes

**Example:**
```rust,ignore
use crate::presentation::table_layout::{Column, Shorten, Table};
use crate::presentation::terminal::stdout_width;

let mut table = Table::new(vec![
    Column::new("ID").pad(3),
    Column::new("Name").shorten(Shorten::End { min: 16 }),
    Column::new("File Path").shorten(Shorten::Path).droppable(1),
])
.ruler('-');
table.row(vec!["1".into(), "Qwen3-4B".into(), path.into()]);
print!("{}", table.lay_out(stdout_width()).text());
```

### Terminal
**Module:** `terminal.rs`

The one place the terminal's width is read, through `console`: `None` when
the stream is piped or redirected. `COLUMNS` is not read.

**Key Functions:**
- `stdout_width()` - Standard output's width
- `stderr_width()` - Standard error's width

### Status Line
**Module:** `status_line.rs`

One line on stderr that each report rewrites on a terminal, and wipes when
what it reports is over; on a pipe each report is its own line. A chat's
tool progress is shown on it, and so is a wait on Hugging Face's rate limit,
which a pipe is told once a wait.

**Key Functions:**
- `show()` / `wipe()` - Write the line, and wipe it
- `hub_wait_notice()` - The notice a one-shot command's Hub client is told a rate-limit wait through

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

- **termimad** - Markdown rendering for Rich mode (`style.rs`)
- **crossterm** - The colours and attributes of that skin (`style.rs`)
- **indicatif** - The spinner (`style.rs`)
- **chrono** - Relative times (`tables.rs`)
- **console** - The terminal's width (`terminal.rs`) and the display width of text (`table_layout.rs`)
- Everything else is ANSI color codes and formatting with `println!`

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
2. **Terminal-Friendly** - A table fits the terminal it is printed in, and is whole when piped (`table_layout.rs`)
3. **Composable** - Small, focused functions that work together
4. **Testable** - Pure functions with no side effects (except printing)

## Future Considerations

- Color theme customization
- Progress bar utilities (currently in individual handlers)

<!-- module-docs:end -->
