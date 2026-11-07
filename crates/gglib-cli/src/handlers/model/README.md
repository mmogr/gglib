# model

<!-- module-docs:start -->

Model management command handlers.

Dispatches [`ModelCommand`] variants to focused handler modules covering
CRUD, verification, download, and `HuggingFace` discovery.

A command here is a process apart from the daemon that serves models. What
is being served, it reads from the pid files under its data root
(`recorded_servers`), which is how `remove` and `upgrade` refuse a model
that is being served and `inspect` says that one is. A pid file says
nothing of the slots a model fills, so `explain` asks the daemon that left
its token under the data root for the launch, planned over what it has
loaded, and plans over nothing when there is none.

What a command changes in the library through `ModelOps`, it tells that
daemon when it is over (`daemon_client::library_changes`), so an app open on
the daemon shows the change. No handler tells anybody for itself.

<!-- module-docs:end -->
