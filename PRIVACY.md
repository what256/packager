# Privacy and network policy

Packager runs on the user's computer. The Packager project does not operate a
backend service, collect analytics, serve advertising, or receive telemetry
from the desktop application or CLI.

## Network activity

Packager connects to other systems only for the following documented product
functions:

- The desktop application checks Packager's GitHub Releases endpoint for a
  signed application update when it starts. If a newer stable version is
  available, the signed updater artifact is downloaded and installed.
- Installing the managed runtime downloads version-pinned Colima, Lima, Docker
  Compose, or Podman assets from their official GitHub release pages. Packager
  verifies every managed asset against a committed SHA-256 digest.
- Installing, starting, or updating a packaged app may contact the container
  registry named by that package to download its images. Automatic image
  updates are enabled by default and can be disabled for each installed app.
- An explicit request to analyze a public GitHub repository downloads that
  repository into Packager's local cache.
- A package may make its own outbound connections as described by that
  upstream application. Those connections are made by third-party software,
  not by a Packager-operated service.
- A package can reach a named service already running on the computer only when
  its reviewed recipe declares that connection. Package web interfaces remain
  bound to the loopback interface.

These requests necessarily expose ordinary connection information, such as the
user's IP address, to the contacted provider. Packager does not add a device
identifier or tracking identifier to them.

## Local information

Package recipes, state, logs, runtime files, and application data stay on the
computer. Generated secrets are stored in macOS Keychain or Windows Credential
Manager. Packager does not upload them.

Uninstalling an app can either preserve or delete its local data, as selected
by the user. `packager runtime uninstall` removes Packager's private runtime
machine and runtime tools. Operating-system package managers may keep their own
download caches according to their policies.

## Reports

The project has no privacy inbox containing user data because no data is
collected. Security or privacy problems can be reported privately through
[GitHub Security Advisories](https://github.com/what256/packager/security/advisories/new).
