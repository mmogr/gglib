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

A message sent with Draw must draw. Pressing the button is the person's
explicit ask, so whether to draw is not left to the model: the run's first
reply is held to a call of `generate_image`, offered alone
(`DrawingTool::first_call`, which the loop reads as `AgentConfig::first_call`),
and every later reply is offered the run's whole tool list and left to
choose. A first reply that calls nothing ends the run with `DRAW_NOT_ASKED`,
"the model did not ask for the picture; try again or pick a model that calls
tools", never with that reply as its answer.

<!-- module-docs:end -->
