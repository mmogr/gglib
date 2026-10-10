# repositories

<!-- module-docs:start -->

Repository implementations using `SQLite`.

These implementations encapsulate all SQL queries and database access.
The `SqlitePool` is confined to this module and never exposed through
the port trait signatures.

`SqliteModelRepository` reads and writes a model's `model_components` rows
beside its own row (`model_component_rows`): every read attaches them, the
listing in one query; a registration adds them without overwriting a role
already linked; an update replaces the set in the transaction that updates
the row, and drops the `model_files` row of any link it removes or points
elsewhere (the registrar's record of a companion, by its absolute path).

<!-- module-docs:end -->
