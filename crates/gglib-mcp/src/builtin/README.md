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
sandbox root, and `generate_image` (`generate_image.rs`) needs a drawing tool
that is armed for this message, which is a run sent with `draw: true` or the
message after `/draw` in `gglib chat`. Listing and calling both read the
table, so a tool that is not listed is refused when called. The page's tool
list (`bare_definitions`) never carries `generate_image`.

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

An empty `size` (`""`, or only spaces) is no size: the picture is drawn at
the image model's default, as when the argument is left out. A size that
says something is refused as before: here when it is not `WIDTHxHEIGHT`,
and by the image driver when it is one the model does not draw (`7x7`).

<!-- module-docs:end -->
