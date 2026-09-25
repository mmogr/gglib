# ports

<!-- module-docs:start -->

Port definitions (trait abstractions) for external systems.

Ports define the interfaces that the core domain expects from infrastructure.
They contain no implementation details and use only domain types.

# Design Rules

- No `sqlx` types in any signature
- No process/filesystem implementation details
- Traits are minimal and CRUD-focused for repositories
- Intent-based methods throughout (not implementation-leaking)

<!-- module-docs:end -->
