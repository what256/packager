# Third-party runtime notices

Packager downloads, verifies, and manages these unmodified open-source runtime tools. They are not embedded in the source repository or installer.

| Component | Pinned version | License | Project |
| --- | ---: | --- | --- |
| Colima | 0.10.1 | MIT | https://github.com/abiosoft/colima |
| Lima | 2.1.1 | Apache-2.0 | https://github.com/lima-vm/lima |
| Docker CLI | 29.4.3 | Apache-2.0 | https://github.com/docker/cli |
| Docker Compose | 5.1.4 | Apache-2.0 | https://github.com/docker/compose |
| Podman (Windows) | 5.8.2 | Apache-2.0 | https://github.com/containers/podman |

Colima, Lima, and Docker CLI are used on macOS. Podman is used on Windows with WSL2. Docker Compose is used on both. Each component remains subject to its own license and notices. Packager's runtime downloader uses architecture-specific HTTPS URLs and committed SHA-256 digests.

Packager also includes Open Notebook's official launcher artwork from the MIT-licensed [Open Notebook repository](https://github.com/lfnovo/open-notebook). It is used only to identify the built-in Open Notebook package; source details are recorded beside the packaged icon files.
