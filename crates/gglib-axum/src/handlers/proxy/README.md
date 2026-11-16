# proxy

<!-- module-docs:start -->

Handlers for the OpenAI-compatible proxy: status, start, start-pinned, stop,
and the loop guard's log, one row per day, model, version and mode
(`GET /api/proxy/loop-guard-trips`).

`wire` holds the request and response shapes and the rules for resolving what a
caller left out — an omitted field means "use what is configured", not "use the
compile-time default", so a tray panel that sends no port must still land on
the same one as `gglib proxy`. Those rules are where the surprises live, and
they are testable without any routing around them. The status names a pin by
its model's catalogue id (`pinned_model_id`) beside its name (`pinned_model`,
for showing): two models can share a name, and a rename keeps the id.

What stays in `mod` is the routing and two decisions:

- **Start** and **start-pinned** go through `ProxyOps::start_as`. An endpoint
  already on as the body asks is success (`outcome: unchanged`), because
  every caller wants an endpoint rather than exclusivity. One on otherwise,
  a different pin, port, address, sampling or cache among them, is left
  alone and refused with `409` and type `endpoint_differs`, whose `metadata`
  says what runs (`now`) and what switching does (`action`), so `gglib
  proxy` and `gglib serve` can ask before they switch. A body with `replace`
  switches it in one operation (`outcome: replaced`); a switch whose start is
  refused puts back what ran and says so. A bind failure on an endpoint
  that was off is still refused with the message that names the port.
- **Stop** treats an already-stopped proxy as success.

<!-- module-docs:end -->
