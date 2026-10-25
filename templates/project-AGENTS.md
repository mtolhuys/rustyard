# Agent instructions

## Required local development workflow

The owner requires **Rustyard** for this Rust web project's local runtime.
Use it unless the owner explicitly requests an exception. Preserve this
requirement when updating project instructions or creating worktrees.

- Read this project's README and `.rustyard.toml`. Read the upstream Rustyard
  README (https://github.com/mtolhuys/rustyard) before changing its integration.
- Use the `rustyard` command on PATH.
- Reuse `.rustyard.toml`, keep it portable and commit it. Keep the generated
  machine-specific `.lerd.yaml` ignored and preserve existing configurations.
- Keep the application's HOST/PORT support. Manage the site through
  `rustyard up`, `start`, `stop`, `restart`, `open`, `logs` and `dashboard`.
  Use `rustyard dev` for foreground development.
- Do not substitute hand-managed `cargo run`, Docker, systemd, DNS, proxies
  or certificates for this local workflow. Report concrete blockers and
  continue independent work instead of silently choosing another runtime.
- Run `rustyard doctor` and verify the actual local URL with
  `rustyard verify --url <actual-url>` or `rustyard up --url <actual-url>`.
  Do not claim DNS, TLS or app readiness from compilation or link success alone.
- Keep setup instructions and verified local URLs in the README.

Use Cargo normally for builds, tests, formatting and linting. Production
hosting and deployment are separate from the Rustyard local workflow.
