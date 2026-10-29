# Ports Implementation

<!-- module-docs:start -->

Port implementations for gglib-runtime.

These implementations provide concrete adapters for the abstract ports
defined in gglib-core. They connect the port interfaces to the actual
runtime infrastructure (`ProcessManager`, a llama-server's HTTP API).
The catalog port is not here: it needs only a model repository, so it is
`gglib_core::services::CatalogPortImpl`.

<!-- module-docs:end -->
