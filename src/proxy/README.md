<!-- module-docs:start -->

# Proxy Module

The proxy module implements an OpenAI-compatible API server that sits between clients and `llama-server` instances. It handles automatic model loading and request routing.

## Architecture

```text
┌─────────────┐      ┌────────────────┐      ┌──────────────────┐
│  API Client │ ───► │  Proxy Server  │ ───► │   Model Router   │
│ (OpenAI SDK)│      │ (Axum Handler) │      │ (Finds/Starts)   │
└─────────────┘      └────────────────┘      └────────┬─────────┘
                                                      │
                                                      ▼
                                             ┌──────────────────┐
                                             │   llama-server   │
                                             │    Instance      │
                                             └──────────────────┘
```

## Components

- **handler.rs**: Contains the Axum route handlers for the OpenAI API endpoints (e.g., `/v1/chat/completions`, `/v1/models`).
- **models.rs**: Defines the data structures for OpenAI API requests and responses.
- **mod.rs**: The main entry point for the proxy service.

## Deep Dive: Model Resolution

The proxy allows clients to request models by name (e.g., "llama-2-7b"). The resolution logic works as follows:

1.  **Exact Match**: Checks if the requested ID matches a model name or alias in the database.
2.  **Fuzzy Match**: If no exact match, it looks for models that contain the requested string.
3.  **Auto-Start**: If the resolved model is not currently running, the proxy asks `GuiBackend` to start it on a free port.
4.  **Routing**: Once running, the proxy forwards the original request to the specific `llama-server` instance and streams the response back to the client.

<!-- module-docs:end -->
