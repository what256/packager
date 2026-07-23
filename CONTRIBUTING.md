# Contributing to Packager

Thank you for helping make local software easier to use.

## Before opening a pull request

1. Keep lifecycle behavior in `packager-core`; desktop and CLI code should remain thin adapters.
2. Treat package recipes as untrusted input. Do not add arbitrary shell execution to schema v1.
3. Bind every published package port explicitly to localhost and use dynamic host-port variables.
4. Preserve user data across stop, update, and non-destructive uninstall operations.
5. Run the complete check sequence documented in `README.md`.

Changes to the Windows managed runtime should also keep `runtime-assets.yml` green on both native architectures. Before a stable release, run `scripts/windows-wsl2-smoke.ps1` on a dedicated Windows host with WSL2 or dispatch the `windows-wsl2-e2e.yml` workflow on a runner labeled `packager-wsl2`, and retain its evidence JSON.

Changes to the macOS managed runtime should run `scripts/macos-runtime-smoke.sh` on a Mac with Apple virtualization support or dispatch `macos-runtime-e2e.yml` on a runner labeled `packager-runtime`. Retain the evidence JSON under `docs/evidence/` when it establishes a new release baseline.

Maintainers should follow the security, dependency, release, and incident procedures in [`docs/MAINTAINER_GUIDE.md`](docs/MAINTAINER_GUIDE.md). User-visible changes belong in [`CHANGELOG.md`](CHANGELOG.md).

Production signing is a separate, manually approved release step. Contributors
cannot request or receive signing credentials. Reviewers and approvers follow
the public [`docs/CODE_SIGNING_POLICY.md`](docs/CODE_SIGNING_POLICY.md).

## Adding a catalog package

- Use official upstream images.
- Pin immutable image digests for reviewed catalog releases when possible.
- Include the upstream license and homepage.
- Declare generated credentials under `secrets`; never put secret values in the recipe, Compose file, state, or logs.
- Put persistent data beneath `${PACKAGER_DATA_DIR}`.
- Confirm clean install, update, stop/start, and uninstall behavior on macOS and Windows where supported.

Security issues should be reported privately according to [`SECURITY.md`](SECURITY.md).
