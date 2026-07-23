# Packager

Packager turns containerized, self-hosted software into ordinary local apps on macOS and Windows. Give it a Compose project, a container image, a public GitHub repository, or an existing `packager.yml`; Packager owns the private runtime, installation, data, ports, start/stop lifecycle, logs, secrets, and image updates.

The desktop application and headless CLI are two clients of the same Rust engine. Their internal executable names are kept distinct so workspace builds cannot overwrite one another. Docker Desktop is not required.

> Status: cross-platform alpha. macOS has a repeatable real-workload runtime gate. Windows x64 and ARM64 pass the full workspace checks, real managed-tool installation smoke tests, and a native CLI lifecycle contract covering WSL detection plus machine create/start/status/stop/restart. Starting an actual WSL2 machine and running a real packaged workload still need end-to-end validation on Windows hardware before the first stable release.

Project records: [current status](docs/STATUS.md) · [maintainer guide](docs/MAINTAINER_GUIDE.md) · [release history](CHANGELOG.md) · [security policy](SECURITY.md) · [privacy policy](PRIVACY.md) · [code-signing policy](docs/CODE_SIGNING_POLICY.md)

## Install Packager

Choose whichever interface fits your workflow.

### Credential-free development previews

Download the permanent [Packager 0.1.0 Development Preview](https://github.com/what256/packager/releases/tag/preview-v0.1.0). It includes macOS DMGs, Windows MSI/NSIS installers, standalone CLIs, five local npm `.tgz` packages, and `SHA256SUMS` for all four supported platform/architecture combinations.

For a newer source revision, maintainers can run the [Preview artifacts workflow](https://github.com/what256/packager/actions/workflows/preview.yml) without Apple, Windows, or npm credentials. Workflow artifacts expire after 14 days; verified builds can be promoted to a permanent GitHub prerelease.

Preview desktop artifacts include `PREVIEW-NOTICE.txt`. macOS builds are completely ad-hoc signed but not Developer ID signed or Apple-notarized; Windows builds are not Authenticode signed. Gatekeeper and SmartScreen warnings are therefore expected. Preview releases are excluded from the stable automatic-update channel and do not replace the signed release process below.

Packager is applying to the SignPath Foundation open-source program for future
Windows Authenticode signatures. See the public
[code-signing policy](docs/CODE_SIGNING_POLICY.md) for release roles, build
integrity, and the exact scope. Approval has not yet been granted, so current
preview artifacts remain unsigned.

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
npm install --global @what256/packager
packager --help
```

The npm launcher installs only the native package for the current machine: macOS Apple Silicon/Intel or Windows ARM64/x64. Node is only the installer/launcher; all Packager behavior lives in the native Rust binary.

### Standalone CLI and package managers

Every release includes standalone `.tar.gz` and `.zip` archives plus `SHA256SUMS`.

Packager's repository acts as an automatically updated Homebrew tap and Scoop bucket. These channels can advance from a credential-free CLI-only release without waiting for desktop signing:

```bash
# macOS
brew tap what256/packager https://github.com/what256/packager
brew install what256/packager/packager

# Windows
scoop bucket add packager https://github.com/what256/packager
scoop install packager/packager
```

Publishing a `cli-v*` CLI prerelease or an ordinary signed desktop release regenerates the checksum-pinned `Formula/packager.rb` and `bucket/packager.json` files from its four standalone CLI archives. Homebrew and Scoop therefore receive later Packager versions through their normal update commands. Each release also retains version-pinned `packager.rb` and `packager.json` assets for direct installation.

CLI-only releases are GitHub prereleases so they cannot replace the ordinary `releases/latest` response consumed by desktop automatic updates. They are stable CLI distribution events: the same source version is built and executed on native macOS and Windows ARM64/x64 runners, then npm, Homebrew, and Scoop advance independently of Apple or Windows desktop-signing credentials.

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
packager runtime uninstall
packager catalog
packager install open-notebook
packager start open-notebook --open
packager logs open-notebook --lines 200
packager update open-notebook
packager stop open-notebook

packager analyze compose ./my-project --json
packager build compose ./my-project \
  --id my-app --name "My App" --port 3000 --secret APP_ENCRYPTION_KEY \
  --icon ./my-app-logo.png
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

On Windows, Packager does not install Docker or Podman Desktop. It stores the portable tools and Podman configuration under Packager's application-data directory, and creates an OS-visible WSL2 machine named `packager-runtime`. If WSL2 is disabled, Packager explains that the user must run `wsl --install` once and restart if Windows requests it. That Windows feature change is the only step that may require administrator approval. `packager runtime uninstall` removes the private WSL2 machine, its container storage, and Packager's portable runtime tools.

On macOS, Packager does not activate or modify the user's global Docker context. Runtime tools, cache, and Docker configuration remain under Packager's Application Support directory. Colima and Lima VM state uses a short, per-installation directory under `~/.packager/r/` so Lima's Unix sockets remain below macOS's fixed path-length limit. `packager runtime uninstall` removes both locations and the private VM.

The word “Docker” can still appear in diagnostics because Compose talks to the private container engine through the Docker-compatible API. On macOS that connection is a local Unix socket under `~/.packager/r/`; it is not Docker Desktop's `~/.docker/run/docker.sock`, it is not exposed to the internet, and users do not need to install Docker. The first app launch takes longer because Packager downloads the pinned runtime once and then downloads that app's container images.

### Real macOS release gate

Run the full shared-engine lifecycle against a disposable nginx workload on a Mac with Apple virtualization support:

```bash
cargo build --locked --release -p packager-cli
PACKAGER="$PWD/target/release/packager" \
EVIDENCE_PATH="$PWD/artifacts/macos-runtime-e2e.json" \
./scripts/macos-runtime-smoke.sh
```

The script uses an isolated temporary Packager data directory unless `PACKAGER_DATA_DIR` is supplied. It installs and starts the private Colima/Lima runtime, builds a Compose package, verifies loopback HTTP, logs, update settings, image recreation, bind-mounted data preservation, stop, and destructive uninstall, then stops the disposable runtime and removes its sandbox. A self-hosted Mac labeled `packager-runtime` can retain the same JSON evidence through the **Real macOS managed-runtime end-to-end** workflow.

### Real Windows release gate

The final Windows runtime gate is executable as one PowerShell 7 command on a Windows 10/11 host with WSL2:

```powershell
./scripts/windows-wsl2-smoke.ps1 `
  -Packager ./target/release/packager.exe `
  -EvidencePath ./artifacts/windows-wsl2-e2e.json
```

It builds and imports a disposable nginx Compose package, writes unique content into Packager's Windows app-data directory, starts Packager's private WSL2/Podman machine, mounts and serves that data on a dynamic loopback port, checks HTTP readiness and logs, toggles automatic updates, verifies the bind-mounted data survives an image pull/recreate, stops, and destructively uninstalls the workload. It preserves a runtime that was already running and otherwise stops it during cleanup. The generated evidence JSON records the Packager/runtime versions, host architecture, image, URL, timestamps, and passed checks.

Maintainers can register a dedicated GitHub Actions runner with the labels `windows` and `packager-wsl2`, then dispatch the **Real Windows WSL2 end-to-end** workflow. Hosted CI parses this script on both Windows x64 and ARM64; the hardware workflow deliberately waits for the separately provisioned WSL2-capable runner.

## What works

- One Tauri-independent `packager-core` engine shared by desktop and CLI
- Desktop GUI, native CLI, npm launcher, standalone binaries, Homebrew formula, and Scoop manifest
- macOS Apple Silicon/Intel and Windows ARM64/x64 build targets
- Managed runtime with no Docker Desktop dependency
- Monthly and change-triggered verification of the real pinned Podman/Compose downloads on native Windows x64 and ARM64 runners
- Native Windows x64/ARM64 lifecycle tests for WSL2 prerequisite handling and private Podman machine orchestration
- Compose folder, image-reference, and public-GitHub package builder
- Built-in Open Notebook package
- Dynamic loopback ports with collision repair
- Secrets in macOS Keychain or Windows Credential Manager
- Start, stop, readiness detection, logs, native packaged-app launchers, and automatic image updates
- macOS launchers are complete app bundles with their own name, bundle identity, process, and Dock icon; every launcher and the Packager Library use the same shared data directory
- Library and Catalog display the package's real logo; Library's three-dot menu can upload a replacement, create an initial-based icon, or restore the original package artwork
- Native launchers use the same chosen logo in Finder/Dock on macOS and the Start menu on Windows, with the Packager icon only as a fallback
- Signed desktop self-updates; signed/notarized macOS and Authenticode-signed Windows release configuration with post-build signature, timestamp, and updater-artifact verification
- Blocking of privileged containers, host namespaces, engine-socket mounts, devices/capabilities, unrestricted host binds, and non-loopback published ports
- Declarative, narrow connections from packaged apps to named services already running on the computer, without enabling host networking

## Package format

A package is a readable, shareable folder:

```text
my-app/
├── packager.yml
├── compose.yml
├── icon.png               # optional portable icon generated by the Builder
├── icon.icns              # optional macOS launcher icon
├── icon.ico               # optional Windows launcher icon
└── optional build context files
```

The Builder looks for common app-icon, logo, and favicon files in Compose folders and public GitHub repositories. It shows the detected logo before packaging and lets you upload an image or create an initial-based icon instead. A portable `icon.png` is converted into `.icns` on macOS and `.ico` on Windows; an existing platform-native icon still takes priority. Open Notebook keeps its first-party bundled icon.

To change an installed app later, open **Library**, use the app card's three-dot menu, and choose **Change logo**. Upload a PNG, JPEG, WebP, or SVG, or create an icon from one or two letters and a color. Packager keeps the package's original artwork separately, so **Restore original** becomes available after a replacement is saved. The selected logo updates the Library and native launcher together.

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
  host_services:
    - name: local-api
      service: app
      port: 11434
      environment: LOCAL_API_BASE
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

`host_services` is for software already running on the user's computer, such as Ollama or LM Studio. Packager injects `http://host.docker.internal:<port>` into the selected Compose service under the declared environment variable. The packaged app can therefore reach that one configured endpoint while its web UI remains bound to `127.0.0.1` and host networking stays disabled. Ordinary outbound HTTPS connections need no declaration and continue to use the runtime's normal internet connection.

For Open Notebook, Packager supplies `OLLAMA_API_BASE=http://host.docker.internal:11434` automatically. A local Ollama server should therefore be configured as `http://host.docker.internal:11434` from inside Open Notebook; `localhost:11434` refers to the Open Notebook container itself. Ollama Cloud should be configured through Open Notebook's OpenAI-compatible provider with base URL `https://ollama.com/v1` and an Ollama API key.

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
| `GITHUB_TOKEN` | Create the draft release and upload assets; supplied by GitHub Actions |

The planned SignPath Foundation workflow will replace the two
`WINDOWS_CERTIFICATE*` repository secrets after the open-source application is
approved. It does not replace the Tauri updater key or Apple's Developer ID
requirements. Details are in the
[code-signing policy](docs/CODE_SIGNING_POLICY.md).

Packager users do not need any publishing secret. Package application secrets are generated locally and live in the operating-system credential vault.

The first registry publication used a temporary bootstrap token. That token is no longer stored in GitHub. All five npm packages trust GitHub user `what256`, repository `packager`, workflow `publish-npm.yml`, and the `npm publish` action through OIDC. Packager 0.1.1 was published through that token-free path with verified registry signatures and SLSA provenance, then installed and executed on native macOS and Windows ARM64/x64 runners. The dedicated workflow can publish the CLI independently of desktop signing and also runs automatically when a stable GitHub release is published. Credential-free preview tarballs remain available through GitHub Releases. The Windows certificate may be added later without changing the application or CLI architecture.

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

The CI workflow runs the full workspace on GitHub's native macOS and Windows runners for ARM64 and x64. The signed desktop release workflow builds both desktop architectures on each OS. A separate credential-free `cli-v*` workflow builds four standalone CLIs and publishes a GitHub prerelease; that event advances the OIDC-only npm packages plus the repository's validated Homebrew and Scoop channels. An ordinary signed desktop release can advance the same CLI channels as well.

## Publishing

The step-by-step operational and recovery runbook is in [`docs/MAINTAINER_GUIDE.md`](docs/MAINTAINER_GUIDE.md). It includes the safe release commands, account-protection rules, CI failure procedure, and weekly maintenance check.

The development updater key was rotated in July 2026. Its ignored private key is `.tauri/packager.key`; the password is stored in the developer's macOS Keychain under service `dev.packager.release`, account `updater-key-password`.

After creating the GitHub repository and adding its remote:

```bash
./scripts/configure-github-secrets.sh
```

That script uploads only the updater key pair. npm publishing uses trusted OIDC and needs no repository token. Add the deferred Apple and Windows secrets listed above before a stable desktop release. The release workflow fails when credentials are absent and independently verifies Developer ID identity/notarization stapling, Authenticode identity/timestamping, and the presence of signed updater artifacts after each build.

The public npm entry point is `@what256/packager`; its four `@what256/packager-{platform}-{arch}` packages are internal native-binary dependencies. All five packages are public and published from the same workflow.

Push a version tag only after CI passes:

```bash
git tag v0.1.0
git push origin v0.1.0
```

To release only the CLI, npm packages, Homebrew formula, and Scoop manifest without desktop-signing credentials, use the matching source version with a namespaced tag:

```bash
git tag cli-v0.1.1
git push origin cli-v0.1.1
```

The resulting GitHub release is deliberately a prerelease, while the npm and package-manager versions are normal releases. This preserves the desktop updater's `releases/latest` contract. The CLI release workflow explicitly dispatches the npm and package-manager workflows after uploading its assets, so publication does not depend on workflow-generated release events triggering other workflows.

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

Packager is MIT licensed. See [`LICENSE`](LICENSE), [`PRIVACY.md`](PRIVACY.md),
[`CONTRIBUTING.md`](CONTRIBUTING.md), and
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
