# builtin

<!-- module-docs:start -->

In-process built-in tool executor.

Implements [`ToolExecutorPort`] for tools that run directly inside the
server process rather than through an external MCP child process.

# Tool-name format

Names are qualified with `"builtin:"` (e.g. `"builtin:get_current_time"`),
matching the convention used by [`crate::tool_executor::McpToolExecutorAdapter`] where names
are qualified with the numeric server id (e.g. `"3:read_file"`).
[`crate::CombinedToolExecutor`] routes calls with the `"builtin:"` prefix here.

# What is offered

One table names every builtin and what it needs: the filesystem tools need a
sandbox root, and are named by `FILE_TOOLS`, which the CLI's `--tools` check
and the daemon's tool filter read too; and `generate_image` (`generate_image.rs`) needs a drawing tool
that is armed for this message, which is a run sent with `draw: true` or the
message after `/draw` in `gglib chat`. Listing and calling both read the
table, so a tool that is not listed is refused when called. The page's tool
list (`bare_definitions`, `GET /api/builtin/tools`) holds only the tools that
need nothing from the run, so it never carries `generate_image` or a file
tool: the server decides when a run is offered those.

The file tools read only under their sandbox root, which the model is told
about as "the working folder"; each one's description, built when the tools
are listed, names that folder by the absolute path the adapter was given.
A path is relative to it; an absolute path
that already lies under it is taken as is, and any other absolute path is
read as relative to it. Every path is canonicalised and refused unless it
stays under the canonical root (`sandboxing.rs`), and `grep_search` follows
a symlink only when its target does too, and never into a folder it is
already inside, so a link to `.` or `..` lists each file once. A refusal
names the path as the model can use it again, what it wrote or the part
under the root of an absolute path inside it, and never says "sandbox" or
the root. A path that names nothing is refused with the next step
(`MISSING`), one that leaves the root says so (`OUTSIDE`), and one that
cannot be read says why. `read_file` cuts a long file between characters. The three run on a
blocking thread, and a call the tool timeout gives up on stops its walk at
the next entry. In a chat's runs the root is its folder
([ADR 0023](../../../../docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md)).

Which image model draws is never the calling model's to say: the tool has no
such argument, and a `model` key in a call is not read. A person's choice
reaches the tool through `DrawWith`, a cell its composer holds (a run's door
sets it once, `gglib chat` changes it with `/draw <model>`), and the tool
sets it as the model of the request it hands the image driver. With none
chosen the request names no model and the driver's default rule draws.

A message sent with Draw makes one call of `generate_image`. Pressing the
button is the person's explicit ask, so whether to draw is not left to the
model: the run's first reply is held to a call of `generate_image`, offered
alone (`DrawingTool::first_call`, which the loop reads as
`AgentConfig::first_call`). A first reply that does not call it, whether it
calls nothing or only some other tool, ends the run with `DRAW_NOT_ASKED`,
"the model did not ask for the picture; try again or pick a model that calls
tools", never with that reply as its answer. Once the tool has run, whether
it drew or refused the call, the model is asked once more with no tools
offered, and that reply ends the run: its text is the answer, a call in it
is not run, and a reply with no text is answered with `DRAW_NO_WORDS`, "The
model said nothing after the drawing tool ran."

That second request ends with one instruction the transcript never holds, a
`user` message placed after the tool's result and sent to the model for that
request alone. A model offered no tools otherwise tends to say it cannot
draw, though the picture is on the tool's row. When the tool drew it is
`DRAW_THEN_SAY`: "The picture has been made by the drawing tool and the
person is looking at it now. Reply in one or two sentences saying what you
asked the tool to draw. Do not apologise, do not say you cannot draw, and do
not call a tool." When the tool answered with an error it is
`DRAW_THEN_SAY_FAILED`: "The drawing tool did not make a picture; its answer
above says why. Tell the person in one or two sentences what went wrong. Do
not call a tool."

The messages after that one are sent without the tool. So that the model
does not deny the picture then, `gglib-core`'s request pipeline adds a note
to a request whose history holds a drawn picture
(`request_pipeline::drawn_note`). It knows one by this tool's qualified name
and by an answer that starts "Drew " and contains "the user can see ", so
those words of `drew_sentence` are held to its constants by a test here.

The answer ends "you cannot". For a chat model that reads images that stops
being true one message later: the completion adapter shows it the most
recent picture on the next message, with a line that says so
(`request_pipeline::shown_picture`, ADR 0018 decision 19). The answer itself
is not rewritten.

An empty `size` (`""`, or only spaces) is no size: the picture is drawn at
the image model's default, as when the argument is left out. A size that
says something is refused as before: here when it is not `WIDTHxHEIGHT`,
and by the image driver when it is one the model does not draw (`7x7`).

<!-- module-docs:end -->
