# Rustyard contributor instructions

- This is a local Rust web development tool, initially using Lerd as a backend.
- Treat README.md as the current scope and runtime contract. Do not advertise a
  standalone Herd replacement, native desktop UI, or validated macOS support yet.
- Keep configuration, backend integration and process management separate.
- Never silently replace existing `.lerd.yaml`, modify another project's `.env`,
  install system services, trust certificates or execute discovered repositories.
- App commands use argv arrays. Shell quoting is only allowed at the Lerd adapter
  boundary; preserve exact arguments and test unusual executable paths.
- Stop entire child process groups on reload/shutdown; do not leak cargo or app
  processes. Preserve meaningful compilation errors and return backend failures.
- Verify with cargo test --locked, cargo fmt --check and cargo clippy --all-targets
  --locked -- -D warnings. Compile the Axum example when changing the app contract.
- For real acceptance, verify HTTPS and Dev Server logs on a real Linux host; compilation
  and mocks alone do not establish working DNS/TLS or app readiness.
