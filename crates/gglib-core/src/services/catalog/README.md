# catalog

<!-- module-docs:start -->

The model catalog port over a model repository.

[`CatalogPortImpl`] answers `ModelCatalogPort` from any `ModelRepository`:
the proxy's list, its name and id resolution, the launch specification a
model is started from, and the template caps a launch observes. It needs
nothing beyond the repository port, the file system and domain functions, so
every composition shares this one implementation, a build without a process
runtime included.

| Module | Contents |
|--------|----------|
| `port` | [`CatalogPortImpl`], the summary projection, [`model_to_launch_spec`] |
| `shards` | [`weights_paths`], every shard of a multi-part GGUF; [`total_model_bytes`] over them; [`resident_bytes`] for everything a launch loads |

Every lookup by identifier goes through `ModelRepository::get_by_identifier`,
so this port and `ModelService` resolve a given string the same way.
[`resident_bytes`] is the one figure a launch budgets with and
`gglib model explain` reports.

<!-- module-docs:end -->
