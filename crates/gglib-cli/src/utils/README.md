# utils

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-complexity.json)

<!-- module-docs:start -->

CLI utility functions for user interaction and input handling.

## Purpose

This module provides low-level utility functions for CLI-specific concerns, primarily focused on user input and terminal interaction. These utilities are used across command handlers to create consistent, user-friendly experiences.

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                      utils Module                            │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Handler → Input Utility → stdin/stdout → User             │
│             (this)                                           │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`input.rs`](input) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-input-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-input-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-input-coverage.json) |
<!-- module-table:end -->

## Components

### Input Handling
**Module:** `input.rs`

Provides functions for prompting users and reading responses from the terminal.

**Key Functions:**

#### `confirm(prompt: &str) -> Result<bool>`
Displays a yes/no prompt and returns the user's choice.

```rust,ignore
use gglib_cli::utils::input;

if input::confirm("Delete this model?")? {
    // User said yes
    delete_model(id)?;
} else {
    println!("Cancelled.");
}
```

**Behavior:**
- Accepts: `y`, `yes`, `Y`, `YES` → `true`
- Accepts: `n`, `no`, `N`, `NO` → `false`
- Invalid input → re-prompts
- Ctrl+C → returns error

**Example Output:**
```text
Delete this model? (y/n): y
Deleting model...
```

#### `prompt(message: &str) -> Result<String>`
Displays a text prompt and returns the user's input.

```rust,ignore
use gglib_cli::utils::input;

let name = input::prompt("Enter model name:")?;
println!("You entered: {}", name);
```

**Behavior:**
- Trims whitespace from input
- Empty input returns empty string (caller validates)
- Ctrl+C → returns error

**Example Output:**
```text
Enter model name: Llama 2 7B Chat
You entered: Llama 2 7B Chat
```

#### `prompt_with_default(message: &str, default: &str) -> Result<String>`
Prompts with a default value shown in brackets.

```rust,ignore
use gglib_cli::utils::input;

let port = input::prompt_with_default("Server port", "8080")?;
// If user presses Enter without typing, returns "8080"
```

**Example Output:**
```text
Server port [8080]: 
Using default: 8080
```

#### `select_from_list<T>(prompt: &str, items: &[T]) -> Result<usize>`
Presents a numbered list and returns the selected index.

```rust,ignore
use gglib_cli::utils::input;

let models = vec!["Llama 2 7B", "Mistral 7B", "GPT-J 6B"];
let choice = input::select_from_list("Select a model:", &models)?;
println!("You selected: {}", models[choice]);
```

**Example Output:**
```text
Select a model:
  1. Llama 2 7B
  2. Mistral 7B
  3. GPT-J 6B
Enter choice (1-3): 2
You selected: Mistral 7B
```

## Usage Patterns

### Destructive Actions
Always confirm before destructive operations:

```rust,ignore
use gglib_cli::utils::input;

pub async fn execute_remove(ctx: &CliContext, id: i64, force: bool) -> Result<()> {
    if !force && !input::confirm("Remove model? This cannot be undone.")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    ctx.remove_model(id).await?;
    println!("Model removed.");
    Ok(())
}
```

### Configuration Prompts
Use defaults for common settings:

```rust,ignore
use gglib_cli::utils::input;

let host = input::prompt_with_default("Server host", "127.0.0.1")?;
let port = input::prompt_with_default("Server port", "8080")?;
let port_num: u16 = port.parse()
    .map_err(|_| anyhow!("Invalid port number"))?;
```

### Interactive Selection
Present choices when multiple options exist:

```rust,ignore
use gglib_cli::utils::input;

let quantizations = vec!["Q4_K_M", "Q5_K_M", "Q8_0"];
if quantizations.len() > 1 {
    let idx = input::select_from_list(
        "Multiple quantizations available. Select one:",
        &quantizations
    )?;
    download_quantization(&quantizations[idx]).await?;
}
```

## Error Handling

All input functions return `anyhow::Result` and handle:
- **IO errors** - Terminal read/write failures
- **EOF** - User presses Ctrl+D
- **Interrupt** - User presses Ctrl+C
- **Invalid input** - Out of range selections, malformed input

```rust,ignore
use gglib_cli::utils::input;

match input::confirm("Continue?") {
    Ok(true) => { /* proceed */ },
    Ok(false) => { /* cancel */ },
    Err(e) => {
        eprintln!("Input error: {}", e);
        return Err(e);
    }
}
```

## Testing

Mock stdin/stdout for testing interactive functions:

```rust,ignore
#[test]
fn test_confirm_yes() {
    let input = b"y\n";
    let mut reader = &input[..];
    
    let result = confirm_with_reader("Proceed?", &mut reader);
    assert_eq!(result.unwrap(), true);
}

#[test]
fn test_confirm_no() {
    let input = b"n\n";
    let mut reader = &input[..];
    
    let result = confirm_with_reader("Proceed?", &mut reader);
    assert_eq!(result.unwrap(), false);
}
```

## Design Notes

1. **Simple & Direct** - No fancy TUI libraries, just stdin/stdout
2. **Consistent Experience** - All prompts follow same patterns
3. **Fail-Safe** - Always validate and handle errors gracefully
4. **Ctrl+C Friendly** - Respect user's desire to exit
5. **No Side Effects** - Functions only read input, don't modify state

## Dependencies

- **std::io** - Terminal I/O operations
- **anyhow** - Error handling

## Future Considerations

- Colorized prompts (green for defaults, yellow for warnings)
- Multi-line input support
- Password input with hidden echo
- Arrow key navigation for select_from_list
- History and autocomplete for repeated prompts

<!-- module-docs:end -->
