# Project status

Last reviewed: 2026-07-18

Packager is a cross-platform alpha. The CLI is publicly distributed; desktop builds remain unsigned development previews until the deferred Apple and Windows credentials are available.

## Available today

- Public npm CLI: `@what256/packager@0.1.1`
- Standalone CLI release: `cli-v0.1.1`
- Homebrew formula and Scoop manifest at version 0.1.1
- Unsigned desktop development preview: `preview-v0.1.0`
- Shared Rust core used by both the desktop app and CLI
- macOS ARM64/x64 and Windows ARM64/x64 builds
- Package analysis, build, import, install, start, readiness, logs, update, stop, and uninstall
- Managed private runtimes that do not require Docker Desktop
- Dedicated packaged-app launchers with package-specific identities and icons, hidden Packager startup UI, and one shared Library/data root
- Builder icon discovery with a visual original-logo preview, image upload, initial-based icon creator, and portable macOS/Windows conversion
- Package-logo rendering in Library and Catalog, plus installed-app upload/create/restore controls that regenerate the native launcher icon
- Declarative host-service connections for packaged apps; the bundled Open Notebook package uses this to reach local Ollama without host networking

## Verification record

| Area | Evidence | Result |
| --- | --- | --- |
| Full workspace | [CI run 29349086970](https://github.com/what256/packager/actions/runs/29349086970) | Passed on macOS and Windows, ARM64 and x64 |
| Windows runtime tools/lifecycle | [Runtime run 29349022011](https://github.com/what256/packager/actions/runs/29349022011) | Passed on native Windows ARM64 and x64 |
| npm publication/install | [npm run 29346933498](https://github.com/what256/packager/actions/runs/29346933498) | Five public packages published and installed on four native targets |
| Homebrew/Scoop | [Channel run 29346935235](https://github.com/what256/packager/actions/runs/29346935235) | Both architectures validated on each operating system |
| Real macOS workload | [`evidence/macos-runtime-e2e-2026-07-14.json`](evidence/macos-runtime-e2e-2026-07-14.json) | nginx lifecycle, updates, persistence, and cleanup passed |

The retained macOS evidence was produced from runtime commit `441df0f` and has SHA-256 `55793ec419fa5ef93116171842d732089ab7b61a98ed8d22a40187f86f3bcadb`.

On 2026-07-18, the current macOS desktop build also completed a manual Open Notebook gate from its generated launcher: it installed and started the private Colima/Lima runtime, pulled both images, migrated the database, returned HTTP successfully on a dynamic loopback port, displayed only the `Open Notebook` window, and used the official Open Notebook icon. The launcher runs from its own full `Open Notebook.app` bundle, macOS exposes a distinct `Open Notebook` Dock item, and both it and `Packager.app` resolve the same shared Library/data root. Existing app data was preserved while the older preview definition and launcher were refreshed, and stale project build bundles were removed from Spotlight so Finder exposes only the two installed apps.

## Security baseline

The 2026-07-14 maintenance check found:

- `npm audit`: zero known vulnerabilities.
- `cargo audit`: zero known vulnerabilities.
- RustSec warnings: 16 unmaintained transitive crates and one unsoundness warning. Most belong to Tauri's Linux-only GTK3 dependency graph, which Packager does not ship, while five retired `unic-*` crates enter through Tauri's URL-pattern dependency. These remain upstream dependencies to monitor; they are not reported by RustSec as active vulnerabilities.
- GitHub secret scanning and push protection: enabled.
- GitHub Dependabot security updates: currently disabled. The weekly Codex review still audits both lockfiles; enabling automatic Dependabot pull requests is an optional repository-setting decision.
- npm trusted publishing: configured for all five packages; no npm publication token is stored in GitHub.

Major npm upgrades are available for Vite, the React Vite plugin, and TypeScript. They are routine upgrade candidates, not security fixes, and should be upgraded in a tested pull request instead of directly on `main`.

## Remaining release work

1. Run the real packaged-workload gate on a Windows 10/11 computer or self-hosted runner with WSL2.
2. Obtain and configure the Apple Developer ID/notarization credentials.
3. Obtain and configure the Windows Authenticode code-signing certificate.
4. Build, verify, and publish the first signed stable desktop release.

Longer-term hardening candidates are a signed community catalog and stronger per-package VM isolation. They are not required for the current CLI alpha but should be reconsidered before declaring the package ecosystem stable.

Operational instructions are in [`MAINTAINER_GUIDE.md`](MAINTAINER_GUIDE.md). Release history is in [`../CHANGELOG.md`](../CHANGELOG.md).
