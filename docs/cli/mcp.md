# gglib mcp

Tool servers. An MCP server is a process gglib starts and talks to; its
tools are offered to the model in `chat`, `q`, the chat page and, when you
say so, a paired device's chats.

<!-- snippet: mcp -->
```console
$ gglib mcp add --name filesystem --type stdio --command npx --args="-y,@modelcontextprotocol/server-filesystem,."
✓ Added MCP server 'filesystem' (id: 2)
$ gglib mcp list
Found 1 MCP server(s):

ID   Name                      Type   Enabled   Remote  Lifecycle   Status     Tools 
-----------------------------------------------------------------------------------
2    filesystem                stdio  yes       no      lazy        stopped    0     
$ gglib mcp test filesystem
Testing connection to 'filesystem'...
✓ Connection successful — 14 tool(s) discovered
   • read_file — Read the complete contents of a file as text. DEPRECATED: U…
   • read_text_file — Read the complete contents of a file from the file system a…
   • read_media_file — Read a file and return it as a base64-encoded content block…
   • read_multiple_files — Read the contents of multiple files simultaneously. This is…
   • write_file — Create a new file or completely overwrite an existing file …
   • edit_file — Make line-based edits to a text file. Each edit replaces ex…
   • create_directory — Create a new directory or ensure a directory exists. Can cr…
   • list_directory — Get a detailed listing of all files and directories in a sp…
   • list_directory_with_sizes — Get a detailed listing of all files and directories in a sp…
   • directory_tree — Get a recursive tree view of files and directories as a JSO…
   • move_file — Move or rename files and directories. Can move files betwee…
   • search_files — Recursively search for files and directories matching a pat…
   • get_file_info — Retrieve detailed metadata about a file or directory. Retur…
   • list_allowed_directories — Returns the list of directories that this server is allowed…
```
<!-- /snippet -->

## `add`, `remove`, `list`

`list` reads each server's configuration from this machine's database, and
its Status and Tools from the daemon, whose copy of a server is the one its
chats use. With no daemon running, those two columns show `—` and a line
under the table says why; `list` never starts a daemon.

- A server is `stdio`: `--command` and `--args` (comma-separated) say what
  to run, `--working-dir`, `--env KEY=VALUE` and `--path-extra` shape its
  process. `sse` (HTTP) servers are not supported yet: `add` refuses one,
  and one already stored is listed as `not supported yet`, is never
  started, and can still be removed.
- A server's name is its own: `add` refuses a name another server already
  has.
- `--lifecycle` decides when gglib spawns the server: `eager` at host init,
  `lazy` on first tool use (the default). A server that should not run is
  disabled (`disable`). A server stored as `manual`, a lifecycle older
  gglib had, opens as a disabled `lazy` one.
- A server you add is not told the folder a chat reads: it reads what its
  own arguments name, from `--working-dir` when one was given, else from
  the working directory of the process that spawns it. Each gglib process
  starts one copy of a server, which every chat in that process shares; the
  daemon's copy serves every chat the daemon runs.

## `enable`, `disable`

`enable` and `disable` say whether a server's tools are included in a chat
at all; a disabled server stays configured. There is no `start` or `stop`:
a lazy server starts on first use, an eager one at init, and `disable`
keeps one out of chats.

## `tools`, `test`

`tools <server>` lists the tools the daemon's copy of a server offers, which
are the ones its chats get. It starts the daemon if none is running, and
starts the server there if it is not running, as a chat's first tool call
would. Of a disabled server it says so and starts nothing. `test <server>`
starts a throwaway copy, lists its tools, and stops it: it is how to see
why a server will not start.

- A server that cannot start says why: `chat` and `q` print one line on
  stderr for each lazy server they go on without (none under `-Q`), and
  that line and `test` end with what the server printed on stderr before
  it exited.

## `allow-remote`, `deny-remote`

`allow-remote <server>...` opens servers to paired devices
([Remote access](../remote.md)), and `deny-remote` closes them, by id or by
a name only one server has; `list` shows each server's `Remote`.

- A change applies from a device's next run, with no restart.
- An SSE server and `builtin` are never opened
  ([ADR 0025](../adr/0025-a-server-is-opened-to-paired-devices-by-a-flag-on-it.md)).

## Flags

`gglib mcp <subcommand> --help` lists every flag.
