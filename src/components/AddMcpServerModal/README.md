# AddMcpServerModal

<!-- module-docs:start -->

Modal dialog for adding and configuring Model Context Protocol (MCP) servers. Handles the server type (a stdio process spawn; an SSE HTTP endpoint is shown as not supported yet and cannot be chosen), template-based quick-start, environment variable management, working directory, and PATH overrides.

## Key Files

| File | Role |
|------|------|
| `ServerTemplatePicker.tsx` | Predefined templates (Tavily, GitHub, Brave Search), each needing an API key, for one-click configuration; reading files is built in |
| `ServerTypeConfig.tsx` | Stdio vs SSE radio selector, with SSE disabled and the reason beside it; command/args/workingDir inputs for stdio mode |
| `EnvVarManager.tsx` | Environment variables: a stored one by its key, read-only, with its value a `SecretField` that never holds it until Show or Copy asks; a new one as key and value inputs. `envSent` is what a save sends, a kept variable without its value |

## Structure

```
AddMcpServerModal
    ├── ServerTemplatePicker   ← pre-fills form from template
    ├── ServerTypeConfig       ← stdio (spawn); SSE (HTTP) shown, not selectable
    └── EnvVarManager          ← environment variable key-value pairs
```

Server state is controlled externally via props; the modal emits a save callback with the complete configuration on submit.

<!-- module-docs:end -->
