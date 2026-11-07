# tools

<!-- module-docs:start -->

Settings › Tools' tool servers (MCP): a row each and the one sheet that adds
and edits them, over the one list `ToolsSection` reads (ST-8). The list is
read when the section opens and again after every change made here; the
daemon announces no change to its tool servers, so there is no Refresh and
one added from a terminal shows the next time the section opens.

A row is the server's name, its state as a shape and a word, and "N tools"
while it runs. Hover or focus shows Test, Start or Stop, Edit and ⋯ Remove…
(asked first); Start is greyed with why for a server not used in chats,
Remove… while it runs, and Test while another row's test runs. A server
gglib cannot run (SSE) offers only Remove…. A stdio server whose command no
longer resolves says "Needs relink" with Fix. The name opens the row: the
command, its tools while running, and Devices may use it where the server
can be opened to paired devices (ADR 0025). How the row's last action went
is said under it, never in a toast.

The sheet offers only a stdio server, which is all the daemon runs: no
connection type and no URL until SSE can be chosen (ST-10). Its quick starts
each need an API key; none reads files, which a chat's folder does (ADR
0023). A stored variable arrives as its key alone and an edit sends it back
without its value (ADR 0027).

| File | Role |
|------|------|
| `ToolServers.tsx` | The list, its one-sentence empty state, + Add a server, and the sheet for an add or an edit |
| `ServerRow.tsx` | One server: name, state, tool count; Test, Start/Stop, Edit, ⋯ Remove… with their greyed reasons; Needs relink and Fix; the opened row; the last action's outcome |
| `useServerActions.ts` | What the row actions do over the one list: the server they are busy on, one test at a time, each outcome kept to its row, Fix reading the list again once it resolves |
| `serverState.ts` | A status from the server routes as a shape and a word (`SERVER_STATE_WORDS`, `FAILED_WORD`), with a failure's reason |
| `ServerSheet.tsx` | The add and edit sheet: quick starts, Name, Command, Arguments, Environment variables, Use in chats, ▸ More (Working directory, Extra PATH, Start when gglib starts); `serverSent`, what a save sends |
| `TemplatePicker.tsx` | The quick starts (`SERVER_TEMPLATES`: Tavily Web Search, GitHub, Brave Search) and the line saying reading files is built in |
| `EnvVarManager.tsx` | The variables: a kept one by its key with its value held (`SecretField`), a typed one masked with an eye; `envSent`, what a save sends for them |
| `AllowRemoteCheckbox.tsx` | Devices may use it: the server's own flag set through its own route; a disabled server said to offer nothing, a refusal said beside the box |

## Words this folder says that Rust does not

The state words "Running", "Starting", "Stopped", "Not supported yet" and
"Failed" (`gglib mcp list` prints the first four in lower case; Rust has no
MCP state word yet); "N tools" / "1 tool"; "Tool servers", "No tool servers
yet. Add one to give chats tools such as web search.", "Add a server",
"Asking gglib"; "Test", "Start", "Stop", "Edit", "More for <name>",
"Remove…", "Remove "<name>"?", "This cannot be undone.", "Remove"; "Not
used in chats: Edit it and tick Use in chats to start it", "Stop it before
removing it", "Another test is running", "Start a throwaway instance, list
its tools, then stop it"; "Needs relink", "Fix", "Find the command's
executable again", "Resolved: <path>", "Could not resolve executable.",
"Suggested fix: <the daemon's>", "Install Node.js if not installed (includes
npm/npx)."; "Connected — N tools offered", "Connection failed — <the
daemon's words>"; "Devices may use it", "Disabled, so it offers no tools",
"Could not change whether paired devices may use it"; the sheet's "Add a
server", "Edit <name>", "Quick start", each template's name and line,
"Reading files is built in: choose a folder in a chat. Every template here
needs an API key.", "Name", "Command", "One executable, found on PATH: npx,
python3, node.", "Arguments", "Separated by spaces.", "Environment
variables", "Add variable", "Use in chats", "Its tools are offered in
chats.", "More", "Working directory", "An absolute path, if it needs one.",
"Extra PATH", "Folders added to its PATH, separated by colons.", "Start when
gglib starts", "Otherwise it starts the first time one of its tools is
used.", "Cancel", "Add", "Save", "Saving…", "A server needs a name", "A
server needs a command", "Command is one executable, such as npx; its
arguments go in Arguments".

<!-- module-docs:end -->
