# clients

Tests for `src/services/clients/` — the modules that talk to a server directly
rather than through `getTransport()`.

`proxyDashboard.test.ts` covers the one client whose target is a *different*
server from the app's own backend: a running proxy's HTTP port. Its subject is
credential handling, because that is what forced the module off the native
`EventSource` (which cannot send an `Authorization` header) and onto
`createSSEStream`. The header must go out when a key is configured and stay off
when one is not — an empty header would break every unauthenticated loopback
setup, which is the default.
