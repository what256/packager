# Changelog

This file records user-visible Packager milestones. Dates use UTC. The current release and remaining work are tracked in [`docs/STATUS.md`](docs/STATUS.md).

## Unreleased

### Added

- A repeatable real macOS managed-runtime gate with retained JSON evidence.
- `packager runtime uninstall` for deleting Packager's private VM, tools, and container storage.
- A maintainer handbook and reusable security/dependency check command.

### Fixed

- Packaged-app launchers now use their package-specific macOS/Windows icons and refresh older bundled installations without deleting app data.
- Opening a packaged app no longer runs duplicate start handlers or leaves Packager's main window visible; launcher errors reveal the main window only when user attention is required.
- macOS launchers target the installed Packager bundle explicitly so stale development URL-handler registrations cannot intercept them.
- macOS runtime state now uses a short per-installation path so Lima sockets stay below Darwin's fixed Unix-socket path limit.
- Managed-runtime removal verifies that Colima's background daemon and VM stop before deleting state.
- Windows removal verifies that the private Podman/WSL2 machine is removed before deleting its tools.

## 0.1.1 — 2026-07-14

The first independently published CLI release.

- Published `@what256/packager` and four native architecture packages to npm through trusted GitHub OIDC publishing with provenance.
- Published standalone macOS ARM64/x64 and Windows ARM64/x64 CLI archives.
- Added automatically updated Homebrew and Scoop channels.
- Separated credential-free CLI releases from signed desktop releases so missing Apple or Windows certificates do not block CLI delivery.
- Added native Windows runtime-asset and lifecycle checks for both x64 and ARM64.
- Added the real WSL2 workload script and self-hosted Windows release gate.
- Added post-build verification for future Apple notarization, Authenticode timestamping, and signed updater artifacts.

Release: <https://github.com/what256/packager/releases/tag/cli-v0.1.1>

## 0.1.0 development preview — 2026-07-14

- Added the shared Tauri-independent Rust engine used by the desktop app and CLI.
- Added package analysis/build/import and install/start/stop/update/logs/uninstall commands.
- Added built-in Open Notebook packaging and automatic application-image updates.
- Added managed Colima/Lima runtime support on macOS and portable Podman/WSL2 support on Windows.
- Added unsigned macOS DMGs, Windows MSI/NSIS installers, standalone CLIs, and local npm tarballs for all four supported targets.

This preview is intentionally not Developer ID signed, Apple-notarized, or Authenticode signed and is excluded from the stable desktop updater channel.

Release: <https://github.com/what256/packager/releases/tag/preview-v0.1.0>
