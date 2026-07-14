# Security policy

Packager recipes can cause containers to run on a user's computer, so package review and isolation are part of the security boundary.

Please do not open a public issue for a vulnerability that could expose host files, secrets, Docker access, or arbitrary command execution. Report it privately to the repository maintainers through GitHub Security Advisories.

Imported recipes are untrusted. Packager validates paths and blocks privileged containers, host namespaces, device/capability access, engine-socket mounts, unrestricted host bind mounts, and ports not bound to loopback. Generated secrets live in macOS Keychain or Windows Credential Manager. The managed runtime remains a shared boundary between packaged apps, so users should still review warnings and only import sources they trust.

Only the bundled Open Notebook recipe is first-party. A signed community catalog and stronger per-package VM isolation remain candidates for a stable release.

Maintainers run the lockfile and account-security checks documented in [`docs/MAINTAINER_GUIDE.md`](docs/MAINTAINER_GUIDE.md). Never include credentials, private keys, recovery codes, or exploit details in a public report or repository artifact.
