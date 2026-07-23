# Code signing policy

This policy covers official Packager release artifacts published from
<https://github.com/what256/packager>.

## Windows signing

Packager is applying to the SignPath Foundation open-source program for
Authenticode signing of its Windows desktop application, installers, and
standalone command-line executables.

For Windows releases signed through that program:

> Free code signing provided by SignPath.io, certificate by SignPath Foundation

Until the application is approved and the signing workflow is enabled, preview
Windows artifacts remain unsigned and are identified as such in their release
notes.

## Release roles

| Role | Members and responsibility |
| --- | --- |
| Authors | Contributors who submit changes through GitHub |
| Reviewer | Project maintainer [`@what256`](https://github.com/what256), who reviews external contributions and release contents |
| Approver | Project maintainer [`@what256`](https://github.com/what256), who manually approves each production signing request |

Maintainers and anyone later granted review or signing authority must protect
both GitHub and SignPath accounts with multi-factor authentication. New role
holders must be added to this policy before they can approve a signing request.

## Source and build integrity

- Official binaries are built only from this public repository.
- Release versions are tied to Git tags, and release builds use committed Rust
  and npm lockfiles.
- GitHub Actions builds the unsigned Windows artifacts. The signing service may
  add only the platform signature; signed output is not rebuilt or modified
  afterward.
- Each production signing request requires manual approval after the source
  revision, version, workflow result, and artifact names have been checked.
- Signed artifact metadata must consistently identify Packager and its release
  version.
- Release checks verify Authenticode identity and timestamping before an
  artifact is published.

The current release and verification procedures are documented in
[`MAINTAINER_GUIDE.md`](MAINTAINER_GUIDE.md).

## Privacy and security

Packager has no hosted backend, advertising, analytics, or telemetry service.
Its documented network behavior is described in the
[privacy policy](../PRIVACY.md).

Security issues that could affect signed artifacts, release credentials, or
users should be reported privately through
[GitHub Security Advisories](https://github.com/what256/packager/security/advisories/new).
The maintainer will suspend releases and request certificate revocation if a
signing key, signing account, or published signed artifact is compromised.

## Licensing

Packager is released under the OSI-approved [MIT License](../LICENSE). Official
signed artifacts are built from the same open-source code and do not contain a
separate proprietary edition.
