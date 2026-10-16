# Security

## Reporting a vulnerability

Report it privately through GitHub: on the
[Security tab](https://github.com/mmogr/gglib/security), choose
**Report a vulnerability**. Please don't open a public issue for it.

Say what you found, the version (`gglib --version`), and how to reproduce it
if you can. Replies come in the report.

## Supported versions

Fixes go into the latest stable release. Older releases and release
candidates are not patched.

## What gglib protects

Everything binds `127.0.0.1` by default and gglib is not built to face the
public internet. The README's [Security](README.md#security) section says
what is guarded and what is not.
