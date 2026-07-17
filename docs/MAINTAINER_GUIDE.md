# Packager maintainer guide

This guide is written for a maintainer who does not need to be a Rust, npm, container, or release-engineering expert. When unsure, stop before publishing or deleting anything and ask Codex to inspect the situation.

## The five things to understand

1. **The repository** is the source of truth. Code and documentation live on GitHub. `main` should stay buildable.
2. **CI** is GitHub's automatic test system. A green check means the tested commit passed; a red check means do not release it.
3. **A release** is a permanent version delivered to users. npm versions cannot be overwritten, so release only after CI passes.
4. **Signing** proves who produced a desktop installer. The updater key, Apple certificate, and Windows certificate are three separate credentials.
5. **The runtime** is the private VM/container engine Packager manages. Application data is separate from the application containers so updates can preserve it.

## Your normal weekly routine

The scheduled Codex maintenance review runs every Monday morning at 09:00 in your local Vienna time. It is read-only: it reports findings but does not merge, publish, rotate credentials, or change settings. You can tell Codex “show,” “pause,” “resume,” “change,” or “delete the Packager weekly maintenance review” whenever you want to manage it.

You can run the same check at any time:

```bash
cd /path/to/Packager
./scripts/maintenance-check.sh
```

For the longer version that also builds and tests everything locally:

```bash
./scripts/maintenance-check.sh --full
```

Read the result in this order:

1. **Known vulnerability**: handle now; do not publish until it is understood.
2. **Failed CI**: inspect and fix before merging or releasing.
3. **Unmaintained/unsound warning**: investigate, but it may be an upstream or unused-platform dependency rather than an exploitable flaw.
4. **New major version**: schedule a tested upgrade; do not update blindly.
5. **Minor/patch update**: still test it, but it is usually lower risk.

Once a month, also run the real macOS workload gate. Run the Windows WSL2 gate whenever a suitable Windows machine is available and before a stable desktop release.

## Everyday Packager commands

```bash
# Check Packager and its private runtime
packager status
packager runtime status

# See installed applications
packager apps

# Start, inspect, update, and stop an application
packager start APP_ID --open
packager logs APP_ID --lines 200
packager update APP_ID
packager stop APP_ID

# Remove an app but preserve its data
packager uninstall APP_ID

# Permanently remove an app and its data
packager uninstall APP_ID --delete-data

# Delete Packager's private VM, container images, and runtime tools
packager runtime uninstall
```

`--delete-data` and `runtime uninstall` are destructive. Stop and check the identifier before using them.

If an error mentions `~/.docker/run/docker.sock`, the running desktop app or generated launcher is stale: current Packager uses its own socket below `~/.packager/r/`. Install the current Packager build and open it once so bundled package definitions and launchers refresh. Do not install Docker Desktop as a workaround. A first launch may legitimately take several minutes while the private runtime and large app images download.

## Repository commands you will use most

```bash
cd /path/to/Packager

# See whether local files have changed
git status

# Download main only when the worktree is clean
git pull --ff-only origin main

# See recent commits
git log --oneline -10

# See recent GitHub test runs
gh run list --repo what256/packager --limit 10

# Inspect the failed steps in one run
gh run view RUN_ID --repo what256/packager --log-failed

# Run the standard local checks
npm ci
npm run check
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Do not use `git reset --hard`, force-push `main`, or delete release tags unless you understand exactly why it is necessary.

## When CI turns red

1. Do not release.
2. Open the failed GitHub Actions run or use `gh run view RUN_ID --log-failed`.
3. Identify the first real error; later failures are often consequences of it.
4. Ask Codex to diagnose the failure and prepare a tested fix.
5. Commit and push the fix, then wait for the complete matrix to turn green.

A cancelled run is not proof that a commit works. A platform-specific green job is not a substitute for the complete matrix.

## Dependency and security updates

JavaScript dependencies are recorded in `package-lock.json`; Rust dependencies are recorded in `Cargo.lock`. These lockfiles make builds repeatable.

Useful read-only checks:

```bash
npm audit --audit-level=high
npm outdated
cargo audit
```

Install the Rust audit command once if it is missing:

```bash
cargo install cargo-audit --locked
```

Never edit a lockfile by hand. Ask Codex to upgrade one dependency group, explain breaking changes, run the full test matrix, and update `CHANGELOG.md` when users are affected.

The managed runtime versions and SHA-256 digests are pinned in `crates/packager-core/src/managed_runtime.rs`. A newer Colima, Lima, Docker CLI, Compose, or Podman release is not automatically safe: update the version and digest together, run the asset workflow on both Windows architectures, and rerun the real workload gates.

## npm, 2FA, and trusted publishing

The public package is `@what256/packager`. Four platform packages contain the native binaries. GitHub Actions publishes all five with npm trusted publishing (OIDC), so normal releases do **not** need an npm token.

Keep these npm account protections:

- Keep two-factor authentication enabled.
- Register at least two security keys/passkeys so one lost device does not lock you out.
- Store npm recovery codes in a password manager, not in this repository.
- Never paste a password, recovery code, private key, or certificate into an issue, commit, chat, or terminal command that may be logged.
- Review the trusted publisher settings if the repository owner/name or `publish-npm.yml` filename changes.

Do not run `npm publish` manually. The workflow builds each native binary on its native operating system, attaches provenance, publishes architecture packages first, publishes the launcher last, and verifies installation on all four targets.

You should not need your phone or a one-time code for each automated release: GitHub's OIDC identity authenticates the publishing workflow. npm may still request your security key when you sign in or change sensitive account/package settings.

## Releasing the CLI

For an ordinary CLI release, ask Codex to prepare the version bump and changelog first. Confirm that `package.json`, `Cargo.toml`, `src-tauri/tauri.conf.json`, and all five npm package files contain the same version, then run:

```bash
node scripts/check-versions.mjs NEW_VERSION
git status --short                 # must show only the intended release changes
git add CHANGELOG.md package.json Cargo.toml Cargo.lock src-tauri/tauri.conf.json npm/*/package.json
git commit -m "Prepare Packager NEW_VERSION"
git push origin main
gh run list --repo what256/packager --branch main --limit 5

# Continue only after CI is green and this prints nothing.
git status --short
git tag cli-vNEW_VERSION
git push origin cli-vNEW_VERSION
```

The tag triggers standalone builds, the GitHub CLI prerelease, npm OIDC publication, and Homebrew/Scoop updates. Watch it with:

```bash
gh run list --repo what256/packager --limit 10
```

If publication fails, do not create a second version immediately. Inspect the run first; the npm workflow is idempotent and can safely recognize packages that were already published.

## Releasing the desktop app

Do not create a `vNEW_VERSION` tag until all Apple, Windows, and updater-signing secrets are configured. A stable desktop tag creates a draft GitHub release and verifies signatures before it can be published.

The credentials are separate:

- Tauri updater private key: signs desktop update archives.
- Apple Developer ID certificate plus notarization credentials: signs and notarizes macOS builds.
- Windows Authenticode certificate: signs and timestamps Windows executables and installers.
- npm: no token; trusted OIDC publishing.

The updater key is intentionally ignored by Git. `scripts/configure-github-secrets.sh` uploads only that updater key and its password. Never add `.tauri/packager.key`, `.p12`, `.pfx`, recovery codes, or passwords to Git.

## If something serious happens

Treat these as urgent:

- A secret or private key appears in Git history, logs, an issue, or chat.
- An npm version or GitHub release appears that you did not create.
- Your GitHub or npm account reports an unknown login or security-key change.
- A vulnerability may let a package access host files, the container-engine socket, credentials, or arbitrary commands.

Then:

1. Stop releases and do not merge speculative fixes.
2. Use GitHub's private Security Advisory flow; do not open a public exploit report.
3. Revoke or rotate the exposed credential at its provider. Deleting it from the latest commit is not enough if it entered Git history.
4. Review GitHub Actions runs, npm package provenance, account sessions, and trusted publishers.
5. Prepare, test, and publish a fixed version. Deprecate a bad npm version rather than trying to overwrite it.
6. Write a short incident record describing impact, actions, and follow-up work without including secret values.

If you are unsure whether an event is serious, treat it as serious until it is understood.

## Backups and access

Keep access to the project independent of one laptop:

- GitHub source and releases are the primary project record.
- Keep npm and GitHub recovery codes in a password manager.
- Keep a second registered security key/passkey in a safe place.
- Back up the Tauri updater private key and future Apple/Windows certificate files in encrypted storage.
- Record certificate expiry dates. An expired signing certificate can block releases even when the code is healthy.

Never put backup credentials in the repository, `docs/`, GitHub issues, or release assets.

## Where to look

- Current state and remaining work: [`STATUS.md`](STATUS.md)
- Release history: [`../CHANGELOG.md`](../CHANGELOG.md)
- User and developer overview: [`../README.md`](../README.md)
- Vulnerability reporting: [`../SECURITY.md`](../SECURITY.md)
- Contribution rules: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
