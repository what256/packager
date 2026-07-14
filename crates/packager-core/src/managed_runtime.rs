use crate::model::ManagedRuntimeStatus;
use crate::Engine;
#[cfg(target_os = "macos")]
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};
use uuid::Uuid;

const RUNTIME_VERSION: &str = "2026.07.1";
#[cfg(target_os = "macos")]
const COLIMA_VERSION: &str = "0.10.1";
#[cfg(target_os = "macos")]
const LIMA_VERSION: &str = "2.1.1";
#[cfg(target_os = "macos")]
const DOCKER_VERSION: &str = "29.4.3";
const COMPOSE_VERSION: &str = "5.1.4";
#[cfg(target_os = "windows")]
const PODMAN_VERSION: &str = "5.8.2";
#[cfg(target_os = "windows")]
const PODMAN_MACHINE: &str = "packager-runtime";

#[derive(Clone, Copy)]
struct Asset {
    url: &'static str,
    sha256: &'static str,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const COLIMA: Asset = Asset {
    url: "https://github.com/abiosoft/colima/releases/download/v0.10.1/colima-Darwin-arm64",
    sha256: "cff716570125444d9560e735d8a23ea50e9f70ca722bb9f44ab456c548425ea3",
};
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const COLIMA: Asset = Asset {
    url: "https://github.com/abiosoft/colima/releases/download/v0.10.1/colima-Darwin-x86_64",
    sha256: "c927d411f70b7b40aced1caeef36cf3b19e0318cfad3606a0292cd488e9c4a0f",
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const LIMA: Asset = Asset {
    url: "https://github.com/lima-vm/lima/releases/download/v2.1.1/lima-2.1.1-Darwin-arm64.tar.gz",
    sha256: "b6b0e6701189cd8c4e549cc39e6d054dc681487798b9b774ad2cbd30c08b2bd8",
};
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const LIMA: Asset = Asset {
    url: "https://github.com/lima-vm/lima/releases/download/v2.1.1/lima-2.1.1-Darwin-x86_64.tar.gz",
    sha256: "2dc5b10aa3a4f26d08c1f3fe83e37e01f85a7d9db0d1d5cb6985b18af96ab07d",
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const DOCKER: Asset = Asset {
    url: "https://download.docker.com/mac/static/stable/aarch64/docker-29.4.3.tgz",
    sha256: "bcc9f5635293e3568f00efc5aa3f537eb347844be9c7acd0b383c1db1e2cd92e",
};
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const DOCKER: Asset = Asset {
    url: "https://download.docker.com/mac/static/stable/x86_64/docker-29.4.3.tgz",
    sha256: "98a5e2935c0ba497cc34c54b73467dbe2e2b2fe91a2c8f89481995b412dbd1f4",
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const COMPOSE: Asset = Asset {
    url: "https://github.com/docker/compose/releases/download/v5.1.4/docker-compose-darwin-aarch64",
    sha256: "4cad7fc67dd089a598a15598ad38d04e6f23bf299846d26b2c572f1f96a7c49f",
};
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const COMPOSE: Asset = Asset {
    url: "https://github.com/docker/compose/releases/download/v5.1.4/docker-compose-darwin-x86_64",
    sha256: "c6f6915295918b59c2848e8978612691fdbbef05cae8cae3b78b10aec3e3dbc7",
};

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const PODMAN: Asset = Asset {
    url: "https://github.com/containers/podman/releases/download/v5.8.2/podman-remote-release-windows_amd64.zip",
    sha256: "1b60aae4bd4879c3932c283d2070bb87116ec1f0ab3a4a98eff7e65f318f9e4e",
};
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const PODMAN: Asset = Asset {
    url: "https://github.com/containers/podman/releases/download/v5.8.2/podman-remote-release-windows_arm64.zip",
    sha256: "06c64d11267232ba21d3e43962c0e9036f24550d87fb811bc995195d82c32fca",
};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const COMPOSE: Asset = Asset {
    url: "https://github.com/docker/compose/releases/download/v5.1.4/docker-compose-windows-x86_64.exe",
    sha256: "e1a8faff28c7433635201a2222171b727f33ecdb0ed367e54d162d00432f39aa",
};
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const COMPOSE: Asset = Asset {
    url: "https://github.com/docker/compose/releases/download/v5.1.4/docker-compose-windows-aarch64.exe",
    sha256: "11992bc5de81d7df994bdf58a4eead433f1c287c15965056a3b372ae38888aaf",
};

#[derive(Clone)]
pub struct RuntimePaths {
    root: PathBuf,
    pub bin: PathBuf,
    lima_dist: PathBuf,
    colima_home: PathBuf,
    lima_home: PathBuf,
    docker_config: PathBuf,
    cache_home: PathBuf,
    apps_root: PathBuf,
}

impl RuntimePaths {
    pub fn from_engine(engine: &Engine) -> Result<Self, String> {
        let data = engine.data_dir();
        Ok(Self {
            root: data.join("runtime"),
            bin: data.join("runtime/bin"),
            lima_dist: data.join("runtime/lima-dist"),
            colima_home: data.join("runtime/colima"),
            lima_home: data.join("runtime/lima"),
            docker_config: data.join("runtime/docker-config"),
            cache_home: data.join("runtime/cache"),
            apps_root: data.join("apps"),
        })
    }

    fn prepare(&self) -> Result<(), String> {
        for directory in [
            &self.root,
            &self.bin,
            &self.lima_dist,
            &self.colima_home,
            &self.lima_home,
            &self.docker_config,
            &self.cache_home,
            &self.apps_root,
            &self.compose_plugin_dir(),
        ] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("Cannot create {}: {error}", directory.display()))?;
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn colima(&self) -> PathBuf {
        self.bin.join("colima")
    }

    #[cfg(target_os = "macos")]
    fn docker(&self) -> PathBuf {
        self.bin.join("docker")
    }

    #[cfg(target_os = "macos")]
    fn limactl(&self) -> PathBuf {
        self.lima_dist.join("bin/limactl")
    }

    fn compose_plugin_dir(&self) -> PathBuf {
        self.docker_config.join("cli-plugins")
    }

    fn compose(&self) -> PathBuf {
        self.compose_plugin_dir()
            .join(if cfg!(target_os = "windows") {
                "docker-compose.exe"
            } else {
                "docker-compose"
            })
    }

    #[cfg(target_os = "macos")]
    fn socket(&self) -> PathBuf {
        self.colima_home.join("packager/docker.sock")
    }

    #[cfg(target_os = "windows")]
    fn podman(&self) -> PathBuf {
        self.bin.join("podman.exe")
    }

    #[cfg(target_os = "windows")]
    fn gvproxy(&self) -> PathBuf {
        self.bin.join("gvproxy.exe")
    }

    #[cfg(target_os = "windows")]
    fn win_sshproxy(&self) -> PathBuf {
        self.bin.join("win-sshproxy.exe")
    }

    fn marker(&self) -> PathBuf {
        self.root.join("runtime-version")
    }

    fn installed(&self) -> bool {
        let marked = fs::read_to_string(self.marker())
            .map(|version| version.trim() == RUNTIME_VERSION)
            .unwrap_or(false);
        #[cfg(target_os = "macos")]
        return marked
            && [self.colima(), self.docker(), self.limactl(), self.compose()]
                .iter()
                .all(|path| path.is_file());
        #[cfg(target_os = "windows")]
        return marked
            && [
                self.podman(),
                self.gvproxy(),
                self.win_sshproxy(),
                self.compose(),
            ]
            .iter()
            .all(|path| path.is_file());
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        return false;
    }

    fn path_env(&self) -> OsString {
        #[cfg(target_os = "macos")]
        {
            let mut paths = vec![self.bin.clone(), self.lima_dist.join("bin")];
            paths.extend(
                ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                    .into_iter()
                    .map(PathBuf::from),
            );
            std::env::join_paths(paths).unwrap_or_else(|_| OsString::from("/usr/bin:/bin"))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut paths = vec![self.bin.clone()];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            std::env::join_paths(paths).unwrap_or_default()
        }
    }

    fn apply_environment(&self, command: &mut Command, include_socket: bool) {
        #[cfg(target_os = "macos")]
        {
            command
                .env("HOME", &self.apps_root)
                .env("COLIMA_HOME", &self.colima_home)
                .env("LIMA_HOME", &self.lima_home)
                .env("DOCKER_CONFIG", &self.docker_config)
                .env("XDG_CACHE_HOME", &self.cache_home)
                .env("PATH", self.path_env());
            if include_socket {
                command.env(
                    "DOCKER_HOST",
                    format!("unix://{}", self.socket().to_string_lossy()),
                );
            }
        }
        #[cfg(target_os = "windows")]
        {
            let config = self.root.join("podman-config");
            let data = self.root.join("podman-data");
            command
                .env("HOME", &self.root)
                .env("APPDATA", &config)
                .env("LOCALAPPDATA", &data)
                .env("XDG_CONFIG_HOME", &config)
                .env("XDG_DATA_HOME", &data)
                .env("XDG_CACHE_HOME", &self.cache_home)
                .env("PODMAN_COMPOSE_PROVIDER", self.compose())
                .env("PATH", self.path_env());
            let _ = include_socket;
        }
    }
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot checksum {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn download(asset: Asset, destination: &Path) -> Result<(), String> {
    if destination.is_file() && sha256(destination)? == asset.sha256 {
        return Ok(());
    }
    let temporary = destination.with_extension(format!("part-{}", Uuid::new_v4()));
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(20 * 60))
        .user_agent(format!("Packager/{RUNTIME_VERSION}"))
        .build()
        .map_err(|error| format!("Cannot prepare runtime download: {error}"))?;
    let mut response = client
        .get(asset.url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Cannot download {}: {error}", asset.url))?;
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("Cannot create {}: {error}", temporary.display()))?;
    response
        .copy_to(&mut file)
        .map_err(|error| format!("Cannot save {}: {error}", destination.display()))?;
    file.flush()
        .map_err(|error| format!("Cannot finish {}: {error}", destination.display()))?;
    let actual = sha256(&temporary)?;
    if actual != asset.sha256 {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Runtime download failed verification (expected {}, got {actual})",
            asset.sha256
        ));
    }
    fs::rename(&temporary, destination)
        .map_err(|error| format!("Cannot install {}: {error}", destination.display()))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("Cannot make {} executable: {error}", path.display()))
}

#[cfg(all(not(unix), target_os = "macos"))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn unpack_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path)
        .map_err(|error| format!("Cannot open {}: {error}", archive_path.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(archive_file));
    for entry in archive
        .entries()
        .map_err(|error| format!("Cannot inspect runtime archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("Invalid runtime archive: {error}"))?;
        let unpacked = entry
            .unpack_in(destination)
            .map_err(|error| format!("Cannot unpack runtime archive: {error}"))?;
        if !unpacked {
            return Err("Runtime archive contains an unsafe path".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn unpack_podman(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path)
        .map_err(|error| format!("Cannot open {}: {error}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Cannot inspect Podman archive: {error}"))?;
    for name in ["podman.exe", "gvproxy.exe", "win-sshproxy.exe"] {
        let index = (0..archive.len())
            .find(|index| {
                archive
                    .by_index(*index)
                    .map(|entry| {
                        entry
                            .name()
                            .replace('\\', "/")
                            .ends_with(&format!("/usr/bin/{name}"))
                    })
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("Podman archive does not contain {name}"))?;
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Cannot read {name} from Podman archive: {error}"))?;
        let destination = destination.join(name);
        let mut output = fs::File::create(&destination)
            .map_err(|error| format!("Cannot create {}: {error}", destination.display()))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("Cannot extract {name}: {error}"))?;
    }
    Ok(())
}

pub fn install(engine: &Engine) -> Result<ManagedRuntimeStatus, String> {
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    return Err("The managed runtime currently supports macOS and Windows".into());

    #[cfg(target_os = "macos")]
    {
        let paths = RuntimePaths::from_engine(engine)?;
        paths.prepare()?;
        let downloads = paths.root.join("downloads");
        fs::create_dir_all(&downloads)
            .map_err(|error| format!("Cannot create runtime downloads: {error}"))?;

        let colima_download = downloads.join(format!("colima-{COLIMA_VERSION}"));
        let lima_download = downloads.join(format!("lima-{LIMA_VERSION}.tar.gz"));
        let docker_download = downloads.join(format!("docker-{DOCKER_VERSION}.tar.gz"));
        let compose_download = downloads.join(format!("compose-{COMPOSE_VERSION}"));
        download(COLIMA, &colima_download)?;
        download(LIMA, &lima_download)?;
        download(DOCKER, &docker_download)?;
        download(COMPOSE, &compose_download)?;

        fs::copy(&colima_download, paths.colima())
            .map_err(|error| format!("Cannot install Colima: {error}"))?;
        fs::copy(&compose_download, paths.compose())
            .map_err(|error| format!("Cannot install Docker Compose: {error}"))?;

        let staging = paths.root.join(format!("staging-{}", Uuid::new_v4()));
        fs::create_dir_all(&staging)
            .map_err(|error| format!("Cannot create runtime staging: {error}"))?;
        let result = (|| {
            let lima_staging = staging.join("lima");
            let docker_staging = staging.join("docker");
            fs::create_dir_all(&lima_staging)
                .and_then(|_| fs::create_dir_all(&docker_staging))
                .map_err(|error| format!("Cannot prepare runtime staging: {error}"))?;
            unpack_archive(&lima_download, &lima_staging)?;
            unpack_archive(&docker_download, &docker_staging)?;
            if paths.lima_dist.exists() {
                fs::remove_dir_all(&paths.lima_dist)
                    .map_err(|error| format!("Cannot replace Lima runtime: {error}"))?;
            }
            fs::rename(&lima_staging, &paths.lima_dist)
                .map_err(|error| format!("Cannot install Lima runtime: {error}"))?;
            fs::copy(docker_staging.join("docker/docker"), paths.docker())
                .map_err(|error| format!("Cannot install Docker CLI: {error}"))?;
            Ok::<(), String>(())
        })();
        let _ = fs::remove_dir_all(&staging);
        result?;

        for executable in [paths.colima(), paths.docker(), paths.compose()] {
            make_executable(&executable)?;
        }
        // Lima ships helper executables plus adjacent templates used during VM creation.
        for executable_dir in [&paths.bin, &paths.lima_dist.join("bin")] {
            for entry in fs::read_dir(executable_dir)
                .map_err(|error| format!("Cannot inspect runtime tools: {error}"))?
            {
                let path = entry
                    .map_err(|error| format!("Cannot read runtime tool: {error}"))?
                    .path();
                if path.is_file() {
                    make_executable(&path)?;
                }
            }
        }
        fs::write(paths.marker(), format!("{RUNTIME_VERSION}\n"))
            .map_err(|error| format!("Cannot record runtime version: {error}"))?;
        status(engine)
    }

    #[cfg(target_os = "windows")]
    {
        let paths = RuntimePaths::from_engine(engine)?;
        paths.prepare()?;
        let downloads = paths.root.join("downloads");
        fs::create_dir_all(&downloads)
            .map_err(|error| format!("Cannot create runtime downloads: {error}"))?;
        let podman_download = downloads.join(format!("podman-{PODMAN_VERSION}.zip"));
        let compose_download = downloads.join(format!("compose-{COMPOSE_VERSION}.exe"));
        download(PODMAN, &podman_download)?;
        download(COMPOSE, &compose_download)?;

        let staging = paths.root.join(format!("staging-{}", Uuid::new_v4()));
        fs::create_dir_all(&staging)
            .map_err(|error| format!("Cannot create runtime staging: {error}"))?;
        let result = (|| {
            unpack_podman(&podman_download, &staging)?;
            for name in ["podman.exe", "gvproxy.exe", "win-sshproxy.exe"] {
                fs::copy(staging.join(name), paths.bin.join(name))
                    .map_err(|error| format!("Cannot install {name}: {error}"))?;
            }
            fs::copy(&compose_download, paths.compose())
                .map_err(|error| format!("Cannot install Docker Compose: {error}"))?;
            Ok::<(), String>(())
        })();
        let _ = fs::remove_dir_all(&staging);
        result?;
        fs::write(paths.marker(), format!("{RUNTIME_VERSION}\n"))
            .map_err(|error| format!("Cannot record runtime version: {error}"))?;
        status(engine)
    }
}

fn checked_output(output: Result<Output, std::io::Error>, action: &str) -> Result<String, String> {
    let output = output.map_err(|error| format!("Cannot {action}: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if stderr.is_empty() {
            if stdout.is_empty() {
                format!("Could not {action}")
            } else {
                stdout
            }
        } else {
            stderr
        })
    }
}

#[cfg(target_os = "windows")]
fn configure_podman_machine_init(command: &mut Command, cpus: &str) {
    command.args([
        "machine",
        "init",
        "--cpus",
        cpus,
        "--memory",
        "6144",
        "--disk-size",
        "60",
        PODMAN_MACHINE,
    ]);
}

fn docker_version(paths: &RuntimePaths) -> Option<String> {
    if !paths.installed() {
        return None;
    }
    #[cfg(target_os = "macos")]
    if !paths.socket().exists() {
        return None;
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new(paths.docker());
    #[cfg(target_os = "windows")]
    let mut command = Command::new(paths.podman());
    paths.apply_environment(&mut command, true);
    #[cfg(target_os = "macos")]
    command.args(["info", "--format", "{{.ServerVersion}}"]);
    #[cfg(target_os = "windows")]
    command.args(["info", "--format", "{{.Version.Version}}"]);
    checked_output(command.output(), "inspect the managed runtime")
        .ok()
        .filter(|version| !version.is_empty())
}

pub fn status(engine: &Engine) -> Result<ManagedRuntimeStatus, String> {
    let paths = RuntimePaths::from_engine(engine)?;
    let installed = paths.installed();
    let version = docker_version(&paths);
    let running = version.is_some();
    Ok(ManagedRuntimeStatus {
        installed,
        running,
        state: if running {
            "running"
        } else if installed {
            "stopped"
        } else {
            "not-installed"
        }
        .into(),
        version: version.map(|value| {
            if cfg!(target_os = "windows") {
                format!("Podman {value} (WSL2)")
            } else {
                format!("Docker Engine {value}")
            }
        }),
        details: if cfg!(target_os = "windows") {
            if running {
                "Packager's private Podman/WSL2 runtime is ready".into()
            } else if installed {
                "The private Podman tools are installed; the WSL2 machine will start when needed"
                    .into()
            } else {
                "Packager will download verified Podman tools and create a private WSL2 machine"
                    .into()
            }
        } else if running {
            "Packager's private runtime is ready".into()
        } else if installed {
            "The private runtime is installed and will start when needed".into()
        } else {
            "The private runtime will be downloaded once, then managed by Packager".into()
        },
    })
}

pub fn start(engine: &Engine) -> Result<ManagedRuntimeStatus, String> {
    let mut current = status(engine)?;
    if current.running {
        return Ok(current);
    }
    if !current.installed {
        current = install(engine)?;
    }
    if current.running {
        return Ok(current);
    }
    let paths = RuntimePaths::from_engine(engine)?;
    paths.prepare()?;
    #[cfg(target_os = "windows")]
    {
        let wsl = checked_output(
            Command::new("wsl.exe").arg("--status").output(),
            "check WSL2",
        )
        .map_err(|error| {
            format!(
                "Windows Subsystem for Linux 2 is required for Packager's private runtime. Run `wsl --install`, restart Windows if requested, then retry. {error}"
            )
        })?;
        let _ = wsl;
        let mut inspect = Command::new(paths.podman());
        paths.apply_environment(&mut inspect, false);
        let machine_exists = inspect
            .args(["machine", "inspect", PODMAN_MACHINE])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !machine_exists {
            let cpus = std::thread::available_parallelism()
                .map(|count| count.get().clamp(2, 4))
                .unwrap_or(4)
                .to_string();
            let mut init = Command::new(paths.podman());
            paths.apply_environment(&mut init, false);
            checked_output(
                {
                    configure_podman_machine_init(&mut init, &cpus);
                    init.output()
                },
                "create Packager's private WSL2 runtime",
            )?;
        }
        let mut start = Command::new(paths.podman());
        paths.apply_environment(&mut start, false);
        checked_output(
            start.args(["machine", "start", PODMAN_MACHINE]).output(),
            "start Packager's private WSL2 runtime",
        )?;
        let ready = status(engine)?;
        if !ready.running {
            return Err("The WSL2 runtime started but Podman did not become ready".into());
        }
        Ok(ready)
    }

    #[cfg(target_os = "macos")]
    {
        let cpus = std::thread::available_parallelism()
            .map(|count| count.get().clamp(2, 4))
            .unwrap_or(4);
        let memory = host_memory_gib()
            .map(|gib| (gib / 2).clamp(4, 8))
            .unwrap_or(6);
        let mut command = Command::new(paths.colima());
        paths.apply_environment(&mut command, false);
        command
            .current_dir(&paths.apps_root)
            .args([
                "start",
                "packager",
                "--runtime",
                "docker",
                "--vm-type",
                "vz",
                "--cpus",
                &cpus.to_string(),
                "--memory",
                &memory.to_string(),
                "--disk",
                "60",
                "--root-disk",
                "20",
                "--activate=false",
                "--ssh-config=false",
                "--save-config",
                "--mount",
            ])
            .arg(format!("{}:w", paths.apps_root.to_string_lossy()));
        checked_output(command.output(), "start Packager's managed runtime")?;
        let ready = status(engine)?;
        if !ready.running {
            return Err("The managed runtime started but Docker did not become ready".into());
        }
        Ok(ready)
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    Err("The managed runtime currently supports macOS and Windows".into())
}

pub fn stop(engine: &Engine) -> Result<ManagedRuntimeStatus, String> {
    let paths = RuntimePaths::from_engine(engine)?;
    if !paths.installed() {
        return status(engine);
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new(paths.colima());
    #[cfg(target_os = "windows")]
    let mut command = Command::new(paths.podman());
    paths.apply_environment(&mut command, false);
    #[cfg(target_os = "macos")]
    command.args(["stop", "packager"]);
    #[cfg(target_os = "windows")]
    command.args(["machine", "stop", PODMAN_MACHINE]);
    checked_output(command.output(), "stop Packager's managed runtime")?;
    status(engine)
}

pub fn ensure_running(engine: &Engine) -> Result<RuntimePaths, String> {
    start(engine)?;
    RuntimePaths::from_engine(engine)
}

pub fn docker_command(engine: &Engine) -> Result<Command, String> {
    let paths = RuntimePaths::from_engine(engine)?;
    if docker_version(&paths).is_none() {
        return Err("Packager's managed runtime is not running".into());
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new(paths.docker());
    #[cfg(target_os = "windows")]
    let mut command = Command::new(paths.podman());
    paths.apply_environment(&mut command, true);
    Ok(command)
}

#[cfg(target_os = "macos")]
fn host_memory_gib() -> Option<usize> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let bytes = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<usize>()
        .ok()?;
    Some(bytes / 1024 / 1024 / 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn pinned_assets_have_sha256_digests() {
        for asset in [COLIMA, LIMA, DOCKER, COMPOSE] {
            assert_eq!(asset.sha256.len(), 64);
            assert!(asset
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit()));
            assert!(asset.url.starts_with("https://"));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn pinned_windows_assets_have_sha256_digests() {
        for asset in [PODMAN, COMPOSE] {
            assert_eq!(asset.sha256.len(), 64);
            assert!(asset
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit()));
            assert!(asset.url.starts_with("https://"));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_runtime_requires_every_managed_executable() {
        let root = std::env::temp_dir().join(format!("packager-runtime-test-{}", Uuid::new_v4()));
        let engine = Engine::new(root.join("data"), root.join("cache"))
            .expect("test engine should be created");
        let paths = RuntimePaths::from_engine(&engine).expect("runtime paths should resolve");
        paths.prepare().expect("runtime paths should be created");
        for executable in [
            paths.podman(),
            paths.gvproxy(),
            paths.win_sshproxy(),
            paths.compose(),
        ] {
            fs::write(executable, b"test").expect("test executable should be created");
        }
        fs::write(paths.marker(), format!("{RUNTIME_VERSION}\n"))
            .expect("runtime marker should be created");
        assert!(paths.installed());

        fs::remove_file(paths.gvproxy()).expect("test helper should be removable");
        assert!(!paths.installed());
        fs::remove_dir_all(root).expect("runtime test directory should be removable");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_runtime_command_uses_private_environment() {
        use std::collections::HashMap;

        let root = std::env::temp_dir().join(format!("packager-env-test-{}", Uuid::new_v4()));
        let engine = Engine::new(root.join("data"), root.join("cache"))
            .expect("test engine should be created");
        let paths = RuntimePaths::from_engine(&engine).expect("runtime paths should resolve");
        let mut command = Command::new(paths.podman());
        paths.apply_environment(&mut command, false);
        let environment = command
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|value| (key.to_os_string(), value.to_os_string()))
            })
            .collect::<HashMap<_, _>>();

        assert_eq!(
            environment.get(std::ffi::OsStr::new("HOME")),
            Some(&paths.root.clone().into_os_string())
        );
        assert_eq!(
            environment.get(std::ffi::OsStr::new("APPDATA")),
            Some(&paths.root.join("podman-config").into_os_string())
        );
        assert_eq!(
            environment.get(std::ffi::OsStr::new("LOCALAPPDATA")),
            Some(&paths.root.join("podman-data").into_os_string())
        );
        assert_eq!(
            environment.get(std::ffi::OsStr::new("PODMAN_COMPOSE_PROVIDER")),
            Some(&paths.compose().into_os_string())
        );
        let configured_path = environment
            .get(std::ffi::OsStr::new("PATH"))
            .expect("PATH should be configured");
        assert_eq!(
            std::env::split_paths(configured_path).next(),
            Some(paths.bin)
        );
        fs::remove_dir_all(root).expect("environment test directory should be removable");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_machine_init_is_private_and_resource_bounded() {
        let mut command = Command::new("podman.exe");
        configure_podman_machine_init(&mut command, "4");
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            arguments,
            [
                "machine",
                "init",
                "--cpus",
                "4",
                "--memory",
                "6144",
                "--disk-size",
                "60",
                PODMAN_MACHINE,
            ]
        );
    }
}
