# build

<!-- module-docs:start -->

Menu construction: one file per submenu, assembled here in the order
macOS shows them — gglib, File, Edit, View, Model, Endpoint, Window, Help.
Each file holds its submenu's `Entry` rows (id, label, key) and builds them;
the items whose state changes (the Model menu, the Endpoint menu) are handed
back as stateful, so `menu::state_sync` can enable and relabel them.

<!-- module-docs:end -->
