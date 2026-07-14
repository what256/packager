# Packager

Packager turns containerized, self-hosted software into ordinary local apps on macOS and Windows. Give it a Compose project, a container image, a public GitHub repository, or an existing `packager.yml`; Packager owns the private runtime, installation, data, ports, start/stop lifecycle, logs, secrets, and image updates.

The desktop application and headless CLI are two clients of the same Rust engine. Their internal executable names are kept distinct so workspace builds cannot overwrite one another. Docker Desktop is not required.

> Status: cross-platform alpha. macOS is runtime-tested locally. Windows x64 and ARM64 pass the full workspace checks on native GitHub runners. The Windows runtime still needs end-to-end validation on Windows hardware before the first stable release.

## Install Packager

Choose whichever interface fits your workflow.

### Credential-free development previews

Download the permanent [Packager 0.1.0 Development Preview](https://github.com/what256/packager/releases/tag/preview-v0.1.0). It includes macOS DMGs, Windows MSI/NSIS installers, standalone CLIs, five local npm `.tgz` packages, and `SHA256SUMS` for all four supported platform/architecture combinations.

For a newer source revision, maintainers can run the [Preview artifacts workflow](https://github.com/what256/packager/actions/workflows/preview.yml) without Apple, Windows, or npm credentials. Workflow artifacts expire after 14 days; verified builds can be promoted to a permanent GitHub prerelease.

Preview desktop artifacts include `PREVIEW-NOTICE.txt`. macOS builds are completely ad-hoc signed but not Developer ID signed or Apple-notarized; Windows builds are not Authenticode signed. Gatekeeper and SmartScreen warnings are therefore expected. Preview releases are excluded from the stable automatic-update channel and do not replace the signed release process below.

The npm tarballs are not published to the npm registry. Download the launcher and the one native package matching your computer, then install both local files together. For example:

```bash
npm install --global \
  ./packager-cli-darwin-arm64-0.1.0-preview.3.tgz \
  ./packager-cli-0.1.0-preview.3.tgz
```

### Desktop app

Official releases provide operating-system-signed installers through GitHub Releases:

- macOS: open the `.dmg` and drag Packager into Applications.
- Windows: run the signed NSIS `.exe` or MSI installer.

The desktop app provides the catalog, visual package builder, app controls, logs, and dedicated app windows. It updates itself using Tauri's signed updater.

### npm CLI

```bash
npm install --global packager-cli
packager --help
```

The npm launcher installs only the native package for the current machine: macOS Apple Silicon/Intel or Windows ARM64/x64. Node is only the installer/launcher; all Packager behavior lives in the native Rust binary.

### Standalone CLI and package managers

Every release includes standalone `.tar.gz` and `.zip` archives plus `SHA256SUMS`.

It also publishes generated, checksum-pinned manifests:

```bash
# macOS, using the packager.rb asset from the chosen release
brew install --formula https://github.com/what256/packager/releases/download/v0.1.0/packager.rb

# Windows, using the packager.json asset from the chosen release
scoop install https://github.com/what256/packager/releases/download/v0.1.0/packager.json
```

Choose the matching release version. Maintainers can move the same generated files into a Homebrew tap or Scoop bucket without changing their contents.

### Build from source

```bash
cargo install --path crates/packager-cli
packager --help
```

## CLI examples

The CLI uses the same data directory as the desktop app, so apps installed in one appear in the other.

```bash
packager status
packager runtime install
packager catalog
packager install open-notebook
packager start open-notebook --open
packager logs open-notebook --lines 200
packager update open-notebook
packager stop open-notebook

packager analyze compose ./my-project --json
packager build compose ./my-project \
  --id my-app --name "My App" --port 3000 --secret APP_ENCRYPTION_KEY
packager import ./shareable-package
packager auto-updates enable my-app
packager auto-updates run
packager uninstall my-app --delete-data
```

Use `--json` with any command for automation. `PACKAGER_DATA_DIR` and `PACKAGER_CACHE_DIR` override the shared storage roots.

## Managed runtimes

Packager downloads only version-pinned assets with committed SHA-256 digests.

| Host | Private runtime | Host prerequisite |
| --- | --- | --- |
| macOS 13+ | Colima 0.10.1, Lima 2.1.1, Docker CLI 29.4.3, Compose 5.1.4 | Apple virtualization support |
| Windows 10 22H2+/11 | Podman 5.8.2 portable client, private Podman machine on WSL2, Compose 5.1.4 | WSL2 Windows feature |

On Windows, Packager does not install Docker or Podman Desktop. It stores the portable tools and Podman configuration under Packager's application-data directory, and creates an OS-visible WSL2 machine named `packager-runtime`. If WSL2 is disabled, Packager explains that the user must run `wsl --install` once and restart if Windows requests it. That Windows feature change is the only step that may require administrator approval.

On macOS, Packager does not activate or modify the user's global Docker context. Its VM, socket, cache, and Docker configuration remain under Packager's Application Support directory.

## What works

- One Tauri-independent `packager-core` engine shared by desktop and CLI
- Desktop GUI, native CLI, npm launcher, standalone binaries, Homebrew formula, and Scoop manifest
- macOS Apple Silicon/Intel and Windows ARM64/x64 build targets
- Managed runtime with no Docker Desktop dependency
- Compose folder, image-reference, and public-GitHub package builder
- Built-in Open Notebook package
- Dynamic loopback ports with collision repair
- Secrets in macOS Keychain or Windows Credential Manager
- Start, stop, readiness detection, logs, deep-link launchers, and automatic image updates
- Signed desktop self-updates; signed/notarized macOS and Authenticode-signed Windows release configuration
- Blocking of privileged containers, host namespaces, engine-socket mounts, devices/capabilities, unrestricted host binds, and non-loopback published ports

## Package format

A package is a readable, shareable folder:

```text
my-app/
├── packager.yml
├── compose.yml
└── optional build context files
```

Minimal recipe:

```yaml
schema_version: 1
id: my-app
name: My App
version: 1.0.0
description: A useful local app.
category: Utilities
homepage: https://example.com
license: MIT
runtime:
  kind: compose
  compose_file: compose.yml
  project_name: packager-my-app
  ports:
    - name: web
      container_port: 3000
      environment: PACKAGER_WEB_PORT
ui:
  port: web
  path: /
secrets:
  - key: APP_ENCRYPTION_KEY
    generate: uuid
updates:
  strategy: compose-pull
  interval_hours: 24
```

Corresponding Compose declarations:

```yaml
services:
  app:
    image: example/my-app:latest
    ports:
      - "127.0.0.1:${PACKAGER_WEB_PORT:?PACKAGER_WEB_PORT is required}:3000"
    environment:
      APP_ENCRYPTION_KEY: ${APP_ENCRYPTION_KEY:?APP_ENCRYPTION_KEY is required}
    volumes:
      - ${PACKAGER_DATA_DIR:?PACKAGER_DATA_DIR is required}/app:/app/data
```

The machine-readable contract is in [`schemas/packager.schema.json`](schemas/packager.schema.json).

## Updates and secrets

There are three independent update paths:

- Packaged apps pull new Compose images on schedule and recreate running services without deleting data.
- Managed runtime assets change only when a Packager release updates the pinned version and digest.
- The desktop app accepts only updater artifacts signed by the private key corresponding to the public key in `tauri.conf.json`.

Those updater keys do not sign the DMG, Windows installer, or npm package. Publishing uses separate credentials:

| Secret | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Desktop automatic-update artifact signatures on macOS and Windows |
| Six `APPLE_*`/`KEYCHAIN_PASSWORD` secrets | Developer ID signing and Apple notarization |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | Authenticode signing for Windows executables/MSI/NSIS |
| `NPM_TOKEN` | Bootstrap the first publication of `packager-cli` and its four native packages; later releases can use npm trusted publishing |
| `GITHUB_TOKEN` | Create the draft release and upload assets; supplied by GitHub Actions |

Packager users do not need any publishing secret. Package application secrets are generated locally and live in the operating-system credential vault.

`NPM_TOKEN` is an npm-account publishing credential, not an API-development credential. It is needed only to bootstrap the first registry publication. The dedicated `Publish npm CLI` workflow can publish and verify the CLI independently of desktop signing. After the packages exist, configure each package's npm trusted publisher for GitHub user `what256`, repository `packager`, workflow `publish-npm.yml`, and the `npm publish` action; the workflow already grants OIDC permission and uses a compatible npm CLI. Credential-free preview tarballs remain available through GitHub Releases. The Windows certificate may be added later without changing the application or CLI architecture.

## Development

Requirements: Node 22+ and current stable Rust. Docker is not a development prerequisite.

```bash
npm install
npm run tauri dev
```

Checks:

```bash
npm run check
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo check -p packager-core -p packager-cli --target x86_64-pc-windows-msvc
```

The CI workflow runs the full workspace on GitHub's native macOS and Windows runners for ARM64 and x64. The release workflow builds both desktop architectures on each OS, signs installers, builds four standalone CLIs, publishes npm packages, and generates package-manager manifests.

## Publishing

The development updater key was rotated in July 2026. Its ignored private key is `.tauri/packager.key`; the password is stored in the developer's macOS Keychain under service `dev.packager.release`, account `updater-key-password`.

After creating the GitHub repository and adding its remote:

```bash
./scripts/configure-github-secrets.sh
```

That script uploads only the updater key pair. Add the Apple, Windows, and npm secrets listed above in repository settings. The workflow deliberately fails instead of creating a partly signed or partly published release when a required credential is missing.

Before the first npm release, confirm the five currently unclaimed package names can be claimed by the npm account represented by `NPM_TOKEN`: `packager-cli` and its four `packager-cli-{platform}-{arch}` packages.

Push a version tag only after CI passes:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag, `Cargo.toml`, `tauri.conf.json`, and root `package.json` versions must match. Releases are created as drafts for human review. Publish them as ordinary releases—not prereleases—because desktop clients use GitHub's `releases/latest` endpoint.

## Architecture

```text
Desktop GUI ─┐
Native CLI ──┼──▶ packager-core ──▶ recipes/state + OS credential vault
npm shim ────┘           │
                         ├── macOS: private Colima/Lima + Docker Compose
                         └── Windows: private Podman/WSL2 + Docker Compose
                                      │
                              app data + loopback UI
```

Packager is MIT licensed. See [`LICENSE`](LICENSE), [`CONTRIBUTING.md`](CONTRIBUTING.md), and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
