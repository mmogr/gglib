# AddMcpServerModal

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-AddMcpServerModal-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-AddMcpServerModal-complexity.json)

<!-- module-docs:start -->

Modal dialog for adding and configuring Model Context Protocol (MCP) servers. Handles server type selection (stdio process spawn vs SSE HTTP endpoint), template-based quick-start, environment variable management, working directory, and PATH overrides.

## Key Files

| File | Role |
|------|------|
| `ServerTemplatePicker.tsx` | Predefined templates (Tavily, Filesystem, GitHub, Brave Search) for one-click configuration |
| `ServerTypeConfig.tsx` | Stdio vs SSE radio selector; command/args/workingDir inputs for stdio mode |
| `EnvVarManager.tsx` | Dynamic key-value editor for environment variables |

## Structure

```
AddMcpServerModal
    ├── ServerTemplatePicker   ← pre-fills form from template
    ├── ServerTypeConfig       ← stdio (spawn) or SSE (HTTP) toggle
    └── EnvVarManager          ← environment variable key-value pairs
```

Server state is controlled externally via props; the modal emits a save callback with the complete configuration on submit.

<!-- module-docs:end -->
