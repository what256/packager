# Contributing to Packager

Thank you for helping make local software easier to use.

## Before opening a pull request

1. Keep lifecycle behavior in `packager-core`; desktop and CLI code should remain thin adapters.
2. Treat package recipes as untrusted input. Do not add arbitrary shell execution to schema v1.
3. Bind every published package port explicitly to localhost and use dynamic host-port variables.
4. Preserve user data across stop, update, and non-destructive uninstall operations.
5. Run the complete check sequence documented in `README.md`.

## Adding a catalog package

- Use official upstream images.
- Pin immutable image digests for reviewed catalog releases when possible.
- Include the upstream license and homepage.
- Declare generated credentials under `secrets`; never put secret values in the recipe, Compose file, state, or logs.
- Put persistent data beneath `${PACKAGER_DATA_DIR}`.
- Confirm clean install, update, stop/start, and uninstall behavior on macOS and Windows where supported.

Security issues should be reported privately according to [`SECURITY.md`](SECURITY.md).
