# ports

<!-- module-docs:start -->

Port definitions (trait abstractions) for external systems.

Ports define the interfaces that the core domain expects from infrastructure.
They hold the traits and the types their signatures carry, plus two
decorators over `ToolExecutorPort` (`tool_executor_filter`) and small
helpers on those types; a few signatures carry a `tokio` channel.

# Design Rules

- No `sqlx` types in any signature
- No process/filesystem implementation details
- Traits are minimal and CRUD-focused for repositories
- Intent-based methods throughout (not implementation-leaking)

<!-- module-docs:end -->
