use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process,
};

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(1);
}

fn state_root() -> PathBuf {
    env::var_os("PACKAGER_RUNTIME_MOCK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| fail("PACKAGER_RUNTIME_MOCK_DIR is required"))
}

fn append_log(root: &Path, executable: &str, arguments: &[String]) {
    fs::create_dir_all(root)
        .unwrap_or_else(|error| fail(&format!("cannot create mock state: {error}")));
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("commands.log"))
        .unwrap_or_else(|error| fail(&format!("cannot open mock log: {error}")));
    writeln!(log, "{} {}", executable, arguments.join(" "))
        .unwrap_or_else(|error| fail(&format!("cannot write mock log: {error}")));
}

fn validate_private_environment() {
    let runtime = env::var_os("PACKAGER_RUNTIME_EXPECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| fail("PACKAGER_RUNTIME_EXPECT_ROOT is required"));
    for (name, expected) in [
        ("APPDATA", runtime.join("podman-config")),
        ("LOCALAPPDATA", runtime.join("podman-data")),
        (
            "PODMAN_COMPOSE_PROVIDER",
            runtime.join("docker-config/cli-plugins/docker-compose.exe"),
        ),
    ] {
        let actual = env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| fail(&format!("{name} is not configured")));
        if actual != expected {
            fail(&format!(
                "{name} escaped the private runtime: expected {}, got {}",
                expected.display(),
                actual.display()
            ));
        }
    }
}

fn run_wsl(root: &Path, arguments: &[String]) {
    append_log(root, "wsl", arguments);
    if arguments != ["--status"] {
        fail("unexpected wsl.exe arguments");
    }
    if env::var_os("PACKAGER_RUNTIME_MOCK_WSL_DISABLED").is_some() {
        fail("WSL2 is not installed");
    }
    println!("Default Version: 2");
}

fn run_podman(root: &Path, arguments: &[String]) {
    validate_private_environment();
    append_log(root, "podman", arguments);
    let machine = root.join("machine-created");
    let running = root.join("machine-running");

    match arguments {
        [command, format, _] if command == "info" && format == "--format" => {
            if running.is_file() {
                println!("5.8.2");
            } else {
                fail("managed Podman machine is stopped");
            }
        }
        [group, command, name]
            if group == "machine" && command == "inspect" && name == "packager-runtime" =>
        {
            if machine.is_file() {
                println!(r#"[{{"Name":"packager-runtime"}}]"#);
            } else {
                fail("managed Podman machine does not exist");
            }
        }
        [group, command, cpus_flag, cpus, memory_flag, memory, disk_flag, disk, name]
            if group == "machine"
                && command == "init"
                && cpus_flag == "--cpus"
                && matches!(cpus.as_str(), "2" | "3" | "4")
                && memory_flag == "--memory"
                && memory == "6144"
                && disk_flag == "--disk-size"
                && disk == "60"
                && name == "packager-runtime" =>
        {
            fs::write(machine, b"created")
                .unwrap_or_else(|error| fail(&format!("cannot create machine marker: {error}")));
        }
        [group, command, name]
            if group == "machine" && command == "start" && name == "packager-runtime" =>
        {
            if !machine.is_file() {
                fail("cannot start a missing managed Podman machine");
            }
            fs::write(running, b"running")
                .unwrap_or_else(|error| fail(&format!("cannot create running marker: {error}")));
        }
        [group, command, name]
            if group == "machine" && command == "stop" && name == "packager-runtime" =>
        {
            if !machine.is_file() {
                fail("cannot stop a missing managed Podman machine");
            }
            if running.is_file() {
                fs::remove_file(running)
                    .unwrap_or_else(|error| fail(&format!("cannot stop mock machine: {error}")));
            }
        }
        _ => fail(&format!(
            "unexpected podman.exe arguments: {}",
            arguments.join(" ")
        )),
    }
}

fn main() {
    let root = state_root();
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let executable = env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|name| name.to_string_lossy().to_lowercase())
        })
        .unwrap_or_default();
    if executable == "wsl" {
        run_wsl(&root, &arguments);
    } else {
        run_podman(&root, &arguments);
    }
}
