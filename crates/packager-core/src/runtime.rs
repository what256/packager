use crate::managed_runtime;
use crate::model::{
    ActionResult, AppSummary, CatalogEntry, InstalledState, PackageRecipe, SystemStatus,
};
use crate::secret_store;
use crate::Engine;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;
use std::{
    collections::HashMap,
    fs,
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Output,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;
use uuid::Uuid;

const OPEN_NOTEBOOK_RECIPE: &str = include_str!("../packages/open-notebook/packager.yml");
const OPEN_NOTEBOOK_COMPOSE: &str = include_str!("../packages/open-notebook/compose.yml");
#[cfg(target_os = "macos")]
const OPEN_NOTEBOOK_ICON: &[u8] = include_bytes!("../packages/open-notebook/icon.icns");
#[cfg(target_os = "windows")]
const OPEN_NOTEBOOK_ICON: &[u8] = include_bytes!("../packages/open-notebook/icon.ico");

#[derive(Clone)]
struct Instance {
    engine: Engine,
    recipe: PackageRecipe,
    state: InstalledState,
    definition_dir: PathBuf,
    data_dir: PathBuf,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_recipe(source: &str) -> Result<PackageRecipe, String> {
    let recipe: PackageRecipe =
        serde_yaml::from_str(source).map_err(|error| format!("Invalid package recipe: {error}"))?;
    validate_recipe(&recipe)?;
    Ok(recipe)
}

pub(crate) fn validate_recipe(recipe: &PackageRecipe) -> Result<(), String> {
    if recipe.schema_version != 1 {
        return Err(format!(
            "Unsupported recipe schema {} (expected 1)",
            recipe.schema_version
        ));
    }
    if recipe.id.is_empty()
        || !recipe.id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err("Package id must contain lowercase letters, numbers, and hyphens only".into());
    }
    if recipe.name.trim().is_empty()
        || recipe.name.len() > 80
        || recipe.version.is_empty()
        || recipe.version.len() > 40
        || !recipe.version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
        || recipe.description.trim().is_empty()
        || recipe.description.len() > 300
        || recipe.category.trim().is_empty()
        || recipe.category.len() > 40
        || recipe.license.trim().is_empty()
    {
        return Err("Package metadata is missing or exceeds schema limits".into());
    }
    let homepage = Url::parse(&recipe.homepage)
        .map_err(|error| format!("Invalid package homepage: {error}"))?;
    if homepage.scheme() != "https" || homepage.host_str().is_none() {
        return Err("Package homepage must be an HTTPS URL".into());
    }
    if recipe.runtime.kind != "compose" {
        return Err(format!("Unsupported runtime kind: {}", recipe.runtime.kind));
    }
    let compose_path = Path::new(&recipe.runtime.compose_file);
    if compose_path.is_absolute()
        || compose_path.components().count() != 1
        || compose_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("compose_file must stay inside the package directory".into());
    }
    if recipe.runtime.project_name.is_empty()
        || !recipe.runtime.project_name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
    {
        return Err("project_name contains unsupported characters".into());
    }
    if recipe.runtime.project_name != format!("packager-{}", recipe.id) {
        return Err("project_name must be packager- followed by the package id".into());
    }
    let valid_variable = |value: &str| {
        !value.is_empty()
            && value.chars().all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
    };
    let mut port_names = std::collections::HashSet::new();
    for port in &recipe.runtime.ports {
        if port.name.is_empty()
            || !port.name.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
            || port.container_port == 0
            || !valid_variable(&port.environment)
            || !port_names.insert(port.name.clone())
        {
            return Err("Runtime ports must have unique safe names, valid container ports, and uppercase environment variables".into());
        }
    }
    match (&recipe.ui.url, &recipe.ui.port) {
        (Some(raw_url), None) => {
            let app_url =
                Url::parse(raw_url).map_err(|error| format!("Invalid app URL: {error}"))?;
            if app_url.scheme() != "http"
                || !matches!(app_url.host_str(), Some("127.0.0.1" | "localhost"))
                || app_url.port().is_none()
            {
                return Err("App URLs must use an explicit localhost HTTP port".into());
            }
        }
        (None, Some(port)) if port_names.contains(port) => {}
        _ => {
            return Err(
                "UI must reference one declared dynamic port or provide one localhost URL".into(),
            )
        }
    }
    if !recipe.ui.path.starts_with('/') || recipe.ui.path.starts_with("//") {
        return Err("UI path must begin with one slash".into());
    }
    let mut secret_keys = std::collections::HashSet::new();
    for secret in &recipe.secrets {
        if !valid_variable(&secret.key)
            || secret.generate != "uuid"
            || !secret_keys.insert(secret.key.clone())
        {
            return Err("Secrets must have unique uppercase keys and a supported generator".into());
        }
    }
    Ok(())
}

fn allocate_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("Cannot allocate a local port: {error}"))
}

fn allocate_unique_port(used: &std::collections::HashSet<u16>) -> Result<u16, String> {
    for _ in 0..32 {
        let port = allocate_port()?;
        if !used.contains(&port) {
            return Ok(port);
        }
    }
    Err("Cannot find a unique local port".into())
}

fn port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn recipe_url(recipe: &PackageRecipe, state: &InstalledState) -> Result<String, String> {
    if let Some(url) = &recipe.ui.url {
        return Ok(url.clone());
    }
    let name = recipe
        .ui
        .port
        .as_ref()
        .ok_or("Package does not declare a UI port")?;
    let port = state
        .ports
        .get(name)
        .ok_or_else(|| format!("Package has no allocated {name} port"))?;
    Ok(format!("http://127.0.0.1:{port}{}", recipe.ui.path))
}

fn prepare_state(
    recipe: &PackageRecipe,
    existing: Option<InstalledState>,
) -> Result<InstalledState, String> {
    let mut state = existing.unwrap_or_else(|| InstalledState {
        installed_version: recipe.version.clone(),
        installed_at: now(),
        automatic_updates: true,
        last_update_check: None,
        environment: HashMap::new(),
        ports: HashMap::new(),
        secret_keys: Vec::new(),
    });
    state.installed_version = recipe.version.clone();
    let mut used_ports = state
        .ports
        .values()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    for port in &recipe.runtime.ports {
        if !state.ports.contains_key(&port.name) {
            let allocated = allocate_unique_port(&used_ports)?;
            used_ports.insert(allocated);
            state.ports.insert(port.name.clone(), allocated);
        }
    }
    for secret in &recipe.secrets {
        if let Some(value) = state.environment.remove(&secret.key) {
            secret_store::set(&recipe.id, &secret.key, &value)?;
        } else if secret_store::get(&recipe.id, &secret.key).is_err() {
            secret_store::set(&recipe.id, &secret.key, &Uuid::new_v4().to_string())?;
        }
        if !state.secret_keys.contains(&secret.key) {
            state.secret_keys.push(secret.key.clone());
        }
    }
    Ok(state)
}

fn apps_root(engine: &Engine) -> Result<PathBuf, String> {
    let root = engine.data_dir().join("apps");
    fs::create_dir_all(&root).map_err(|error| format!("Cannot create app storage: {error}"))?;
    Ok(root)
}

fn instance_root(engine: &Engine, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err("Invalid app id".into());
    }
    Ok(apps_root(engine)?.join(id))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Cannot serialize state: {error}"))?;
    fs::write(path, contents).map_err(|error| format!("Cannot write {}: {error}", path.display()))
}

fn validate_compose_security(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if source.len() > 2 * 1024 * 1024 {
        return Err("Compose file exceeds the 2 MB safety limit".into());
    }
    let compose: serde_yaml::Value =
        serde_yaml::from_str(&source).map_err(|error| format!("Invalid Compose file: {error}"))?;
    let key = |value: &str| serde_yaml::Value::String(value.into());
    let services = compose
        .as_mapping()
        .and_then(|mapping| mapping.get(key("services")))
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or("Compose file must define services")?;
    for section in ["volumes", "networks"] {
        if let Some(resources) = compose
            .as_mapping()
            .and_then(|mapping| mapping.get(key(section)))
            .and_then(serde_yaml::Value::as_mapping)
        {
            for (raw_name, raw_resource) in resources {
                let name = raw_name.as_str().unwrap_or("unnamed");
                if let Some(resource) = raw_resource.as_mapping() {
                    let external = resource
                        .get(key("external"))
                        .and_then(serde_yaml::Value::as_bool)
                        == Some(true);
                    if external || resource.contains_key(key("name")) {
                        return Err(format!(
                            "Compose {section} resource {name} bypasses package namespacing"
                        ));
                    }
                }
            }
        }
    }
    for (raw_name, raw_service) in services {
        let name = raw_name.as_str().unwrap_or("unnamed service");
        let service = raw_service
            .as_mapping()
            .ok_or_else(|| format!("Service {name} must be an object"))?;
        if service
            .get(key("privileged"))
            .and_then(serde_yaml::Value::as_bool)
            == Some(true)
        {
            return Err(format!(
                "Service {name} requests privileged mode, which Packager blocks"
            ));
        }
        for field in ["container_name", "volumes_from"] {
            if service.contains_key(key(field)) {
                return Err(format!(
                    "Service {name} sets {field}, which can escape package namespacing"
                ));
            }
        }
        for field in ["network_mode", "pid", "ipc"] {
            if service.get(key(field)).and_then(serde_yaml::Value::as_str) == Some("host") {
                return Err(format!(
                    "Service {name} requests host {field}, which Packager blocks"
                ));
            }
        }
        for field in ["devices", "cap_add"] {
            if service
                .get(key(field))
                .and_then(serde_yaml::Value::as_sequence)
                .is_some_and(|values| !values.is_empty())
            {
                return Err(format!(
                    "Service {name} requests {field}, which Packager blocks"
                ));
            }
        }
        if let Some(volumes) = service
            .get(key("volumes"))
            .and_then(serde_yaml::Value::as_sequence)
        {
            for volume in volumes {
                let host_source = if let Some(value) = volume.as_str() {
                    value.split(':').next().unwrap_or_default()
                } else {
                    volume
                        .as_mapping()
                        .and_then(|mapping| mapping.get(key("source")))
                        .and_then(serde_yaml::Value::as_str)
                        .unwrap_or_default()
                };
                if host_source.contains("docker.sock") || host_source.contains("containerd.sock") {
                    return Err(format!(
                        "Service {name} tries to mount the container engine socket"
                    ));
                }
                if host_source.starts_with('/')
                    || ((host_source.contains("${") || host_source.contains("$HOME"))
                        && !host_source.starts_with("${PACKAGER_DATA_DIR"))
                {
                    return Err(format!(
                        "Service {name} has an unrestricted host bind mount ({host_source}); use ${{PACKAGER_DATA_DIR}} for app data or a relative package path"
                    ));
                }
            }
        }
        if let Some(ports) = service
            .get(key("ports"))
            .and_then(serde_yaml::Value::as_sequence)
        {
            for port in ports {
                let loopback_only = if let Some(value) = port.as_str() {
                    value.starts_with("127.0.0.1:") || value.starts_with("localhost:")
                } else if let Some(mapping) = port.as_mapping() {
                    matches!(
                        mapping
                            .get(key("host_ip"))
                            .and_then(serde_yaml::Value::as_str),
                        Some("127.0.0.1" | "localhost")
                    )
                } else {
                    false
                };
                if !loopback_only {
                    return Err(format!(
                        "Service {name} publishes a non-loopback port; bind it explicitly to 127.0.0.1"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn load_instance(engine: &Engine, id: &str) -> Result<Instance, String> {
    let root = instance_root(engine, id)?;
    let definition_dir = root.join("definition");
    let recipe_source = fs::read_to_string(definition_dir.join("packager.yml"))
        .map_err(|_| format!("{id} is not installed"))?;
    let recipe = parse_recipe(&recipe_source)?;
    let state_source = fs::read_to_string(root.join("state.json"))
        .map_err(|error| format!("Cannot read state for {id}: {error}"))?;
    let state = serde_json::from_str(&state_source)
        .map_err(|error| format!("Cannot parse state for {id}: {error}"))?;
    Ok(Instance {
        engine: engine.clone(),
        recipe,
        state,
        definition_dir,
        data_dir: root.join("data"),
    })
}

fn save_state(engine: &Engine, id: &str, state: &InstalledState) -> Result<(), String> {
    write_json(&instance_root(engine, id)?.join("state.json"), state)
}

fn builtin_recipes() -> Result<Vec<PackageRecipe>, String> {
    Ok(vec![parse_recipe(OPEN_NOTEBOOK_RECIPE)?])
}

fn launcher_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || *character == ' '
                || *character == '-'
                || *character == '_'
        })
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        "Packaged App".into()
    } else {
        cleaned
    }
}

#[cfg(target_os = "macos")]
fn launcher_bundle(recipe: &PackageRecipe) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Applications")
            .join(format!("{}.app", launcher_name(&recipe.name))),
    )
}

#[cfg(target_os = "windows")]
fn launcher_bundle(recipe: &PackageRecipe) -> Option<PathBuf> {
    let app_data = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(app_data)
            .join("Microsoft/Windows/Start Menu/Programs/Packager Apps")
            .join(format!("{}.lnk", launcher_name(&recipe.name))),
    )
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn launcher_bundle(_recipe: &PackageRecipe) -> Option<PathBuf> {
    None
}

#[cfg(target_os = "macos")]
fn write_launcher_bundle(
    recipe: &PackageRecipe,
    bundle: &Path,
    launcher_icon: &[u8],
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let contents = bundle.join("Contents");
    let executable_dir = contents.join("MacOS");
    let resources_dir = contents.join("Resources");
    fs::create_dir_all(&executable_dir)
        .and_then(|_| fs::create_dir_all(&resources_dir))
        .map_err(|error| format!("Cannot create launcher app: {error}"))?;

    let executable = executable_dir.join("launch");
    let script = format!(
        "#!/bin/sh\n/usr/bin/open -b dev.packager.desktop 'packager://open/{}'\n",
        recipe.id
    );
    fs::write(&executable, script)
        .and_then(|_| fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)))
        .map_err(|error| format!("Cannot write launcher executable: {error}"))?;
    fs::write(resources_dir.join("AppIcon.icns"), launcher_icon)
        .map_err(|error| format!("Cannot write launcher icon: {error}"))?;

    let bundle_id = recipe.id.replace('-', ".");
    let display_name = launcher_name(&recipe.name);
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key><string>{display_name}</string>
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIdentifier</key><string>dev.packager.launcher.{bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>{display_name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{}</string>
  <key>LSMinimumSystemVersion</key><string>10.15</string>
</dict>
</plist>
"#,
        recipe.version
    );
    fs::write(contents.join("Info.plist"), plist)
        .map_err(|error| format!("Cannot write launcher metadata: {error}"))?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn current_app_bundle() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let contents = executable.parent()?.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    (bundle.extension()?.to_str()? == "app").then(|| bundle.to_path_buf())
}

#[cfg(target_os = "macos")]
fn plist_replace(plist: &Path, key: &str, value: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-replace", key, "-string", value])
        .arg(plist)
        .output()
        .map_err(|error| format!("Cannot update launcher metadata: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot update launcher metadata: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(target_os = "macos")]
fn clone_launcher_bundle(
    recipe: &PackageRecipe,
    source: &Path,
    bundle: &Path,
    launcher_icon: &[u8],
) -> Result<(), String> {
    let parent = bundle.parent().ok_or("Invalid launcher app path")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create Applications directory: {error}"))?;
    let staging = parent.join(format!(".packager-launcher-{}.app", Uuid::new_v4()));
    let install = || -> Result<(), String> {
        let output = Command::new("/usr/bin/ditto")
            .arg(source)
            .arg(&staging)
            .output()
            .map_err(|error| format!("Cannot copy Packager application: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Cannot copy Packager application: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let contents = staging.join("Contents");
        let resources = contents.join("Resources");
        let plist = contents.join("Info.plist");
        let display_name = launcher_name(&recipe.name);
        let bundle_id = format!("dev.packager.launcher.{}", recipe.id.replace('-', "."));
        plist_replace(&plist, "CFBundleDisplayName", &display_name)?;
        plist_replace(&plist, "CFBundleName", &display_name)?;
        plist_replace(&plist, "CFBundleIdentifier", &bundle_id)?;
        plist_replace(&plist, "CFBundleIconFile", "AppIcon")?;
        let _ = Command::new("/usr/bin/plutil")
            .args(["-remove", "CFBundleURLTypes"])
            .arg(&plist)
            .output();
        fs::write(resources.join("packager-launcher-id"), &recipe.id)
            .map_err(|error| format!("Cannot identify launcher application: {error}"))?;
        if !launcher_icon.is_empty() {
            fs::write(resources.join("AppIcon.icns"), launcher_icon)
                .map_err(|error| format!("Cannot write launcher icon: {error}"))?;
        }

        let output = Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&staging)
            .output()
            .map_err(|error| format!("Cannot sign launcher application: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Cannot sign launcher application: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        if bundle.exists() {
            fs::remove_dir_all(bundle)
                .map_err(|error| format!("Cannot replace launcher application: {error}"))?;
        }
        fs::rename(&staging, bundle)
            .map_err(|error| format!("Cannot install launcher application: {error}"))?;
        Ok(())
    };

    let result = install();
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(target_os = "macos")]
fn install_launcher(
    recipe: &PackageRecipe,
    definition_dir: &Path,
    fallback_icon: Option<&[u8]>,
) -> Result<(), String> {
    let bundle = launcher_bundle(recipe).ok_or("Cannot locate the user home directory")?;
    let native_icon = definition_dir.join("icon.icns");
    if !native_icon.is_file() && definition_dir.join("icon.png").is_file() {
        create_macos_icon(&definition_dir.join("icon.png"), &native_icon)?;
    }
    let package_icon = fs::read(native_icon).ok();
    let launcher_icon = package_icon
        .as_deref()
        .or(fallback_icon)
        .unwrap_or_default();
    if let Some(source) = current_app_bundle().filter(|source| source != &bundle) {
        clone_launcher_bundle(recipe, &source, &bundle, launcher_icon)?;
    } else {
        write_launcher_bundle(recipe, &bundle, launcher_icon)?;
        let _ = Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&bundle)
            .output();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn create_macos_icon(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination.parent().ok_or("Invalid macOS icon path")?;
    let iconset = parent.join(format!(".packager-icon-{}.iconset", Uuid::new_v4()));
    fs::create_dir_all(&iconset)
        .map_err(|error| format!("Cannot prepare macOS app icon: {error}"))?;
    let result = (|| -> Result<(), String> {
        for (name, size) in [
            ("icon_16x16.png", 16),
            ("icon_16x16@2x.png", 32),
            ("icon_32x32.png", 32),
            ("icon_32x32@2x.png", 64),
            ("icon_128x128.png", 128),
            ("icon_128x128@2x.png", 256),
            ("icon_256x256.png", 256),
            ("icon_256x256@2x.png", 512),
            ("icon_512x512.png", 512),
            ("icon_512x512@2x.png", 1024),
        ] {
            let output = Command::new("/usr/bin/sips")
                .args(["-z", &size.to_string(), &size.to_string()])
                .arg(source)
                .arg("--out")
                .arg(iconset.join(name))
                .output()
                .map_err(|error| format!("Cannot resize macOS app icon: {error}"))?;
            if !output.status.success() {
                return Err(format!(
                    "Cannot resize macOS app icon: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        let output = Command::new("/usr/bin/iconutil")
            .args(["-c", "icns"])
            .arg(&iconset)
            .arg("-o")
            .arg(destination)
            .output()
            .map_err(|error| format!("Cannot create macOS app icon: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Cannot create macOS app icon: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    })();
    let _ = fs::remove_dir_all(iconset);
    result
}

#[cfg(target_os = "windows")]
fn install_launcher(
    recipe: &PackageRecipe,
    definition_dir: &Path,
    _fallback_icon: Option<&[u8]>,
) -> Result<(), String> {
    let shortcut = launcher_bundle(recipe).ok_or("Cannot locate the Windows Start Menu")?;
    let parent = shortcut
        .parent()
        .ok_or("Invalid Start Menu shortcut path")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create Packager Apps Start Menu folder: {error}"))?;
    let executable = std::env::current_exe()
        .map_err(|error| format!("Cannot locate the Packager executable: {error}"))?;
    let icon = definition_dir.join("icon.ico");
    if !icon.is_file() && definition_dir.join("icon.png").is_file() {
        create_windows_icon(&definition_dir.join("icon.png"), &icon)?;
    }
    let script = concat!(
        "$shell = New-Object -ComObject WScript.Shell; ",
        "$link = $shell.CreateShortcut($env:PACKAGER_SHORTCUT); ",
        "$link.TargetPath = $env:PACKAGER_EXE; ",
        "$link.Arguments = $env:PACKAGER_ARGUMENTS; ",
        "$link.WorkingDirectory = Split-Path $env:PACKAGER_EXE; ",
        "if (Test-Path $env:PACKAGER_ICON) { $link.IconLocation = $env:PACKAGER_ICON }; ",
        "$link.Save()"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("PACKAGER_SHORTCUT", &shortcut)
        .env("PACKAGER_EXE", executable)
        .env(
            "PACKAGER_ARGUMENTS",
            format!("\"packager://open/{}\"", recipe.id),
        )
        .env("PACKAGER_ICON", icon)
        .output()
        .map_err(|error| format!("Cannot create Windows app shortcut: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot create Windows app shortcut: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(target_os = "windows")]
fn create_windows_icon(source: &Path, destination: &Path) -> Result<(), String> {
    let decoded = image::open(source)
        .map_err(|error| format!("Cannot read portable app icon: {error}"))?
        .to_rgba8();
    let mut directory = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let resized =
            image::imageops::resize(&decoded, size, size, image::imageops::FilterType::Lanczos3);
        let image = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        let entry = ico::IconDirEntry::encode(&image)
            .map_err(|error| format!("Cannot encode Windows app icon: {error}"))?;
        directory.add_entry(entry);
    }
    let file = fs::File::create(destination)
        .map_err(|error| format!("Cannot create Windows app icon: {error}"))?;
    directory
        .write(file)
        .map_err(|error| format!("Cannot write Windows app icon: {error}"))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn install_launcher(
    _recipe: &PackageRecipe,
    _definition_dir: &Path,
    _fallback_icon: Option<&[u8]>,
) -> Result<(), String> {
    Ok(())
}

fn remove_launcher(recipe: &PackageRecipe) -> Result<(), String> {
    if let Some(bundle) = launcher_bundle(recipe) {
        if bundle.exists() {
            if bundle.is_dir() {
                fs::remove_dir_all(bundle)
                    .map_err(|error| format!("Cannot remove launcher app: {error}"))?;
            } else {
                fs::remove_file(bundle)
                    .map_err(|error| format!("Cannot remove launcher shortcut: {error}"))?;
            }
        }
    }
    Ok(())
}

pub fn catalog(engine: &Engine) -> Result<Vec<CatalogEntry>, String> {
    let root = apps_root(engine)?;
    builtin_recipes()?
        .into_iter()
        .map(|recipe| {
            let installed = root.join(&recipe.id).join("state.json").exists();
            Ok(CatalogEntry {
                id: recipe.id,
                name: recipe.name,
                version: recipe.version,
                description: recipe.description,
                category: recipe.category,
                homepage: recipe.homepage,
                license: recipe.license,
                memory_mb: recipe.requirements.memory_mb,
                disk_mb: recipe.requirements.disk_mb,
                installed,
            })
        })
        .collect()
}

pub fn install(engine: &Engine, id: &str) -> Result<ActionResult, String> {
    let recipe = builtin_recipes()?
        .into_iter()
        .find(|recipe| recipe.id == id)
        .ok_or_else(|| format!("Unknown catalog app: {id}"))?;
    let root = instance_root(engine, id)?;
    let definition_dir = root.join("definition");
    let data_dir = root.join("data");
    fs::create_dir_all(&definition_dir)
        .and_then(|_| fs::create_dir_all(&data_dir))
        .map_err(|error| format!("Cannot create app directories: {error}"))?;

    fs::write(definition_dir.join("packager.yml"), OPEN_NOTEBOOK_RECIPE)
        .and_then(|_| fs::write(definition_dir.join("compose.yml"), OPEN_NOTEBOOK_COMPOSE))
        .map_err(|error| format!("Cannot install package definition: {error}"))?;
    #[cfg(target_os = "macos")]
    fs::write(definition_dir.join("icon.icns"), OPEN_NOTEBOOK_ICON)
        .map_err(|error| format!("Cannot install package icon: {error}"))?;
    #[cfg(target_os = "windows")]
    fs::write(definition_dir.join("icon.ico"), OPEN_NOTEBOOK_ICON)
        .map_err(|error| format!("Cannot install package icon: {error}"))?;

    let existing_state = fs::read_to_string(root.join("state.json"))
        .ok()
        .and_then(|source| serde_json::from_str::<InstalledState>(&source).ok());
    let state = prepare_state(&recipe, existing_state)?;
    write_json(&root.join("state.json"), &state)?;
    if engine.launchers_enabled() {
        install_launcher(&recipe, &definition_dir, engine.launcher_icon())?;
    }

    Ok(ActionResult {
        id: id.into(),
        status: "stopped".into(),
        message: format!(
            "{} is installed in your Applications folder and ready to start.",
            recipe.name
        ),
    })
}

/// Refresh bundled package definitions and native launchers while preserving app data.
/// This also migrates installations created by older Packager previews.
pub fn refresh_installed_packages(engine: &Engine) -> Result<(), String> {
    let root = apps_root(engine)?;
    for recipe in builtin_recipes()? {
        if root.join(&recipe.id).join("state.json").is_file() {
            install(engine, &recipe.id)?;
        }
    }
    Ok(())
}

fn compose_output(instance: &Instance, arguments: &[&str]) -> Result<Output, String> {
    let compose_file = instance
        .definition_dir
        .join(&instance.recipe.runtime.compose_file);
    let mut command = managed_runtime::docker_command(&instance.engine)?;
    command
        .current_dir(&instance.definition_dir)
        .arg("compose")
        .arg("-f")
        .arg(&compose_file)
        .arg("--project-name")
        .arg(&instance.recipe.runtime.project_name)
        .args(arguments)
        .env("PACKAGER_DATA_DIR", &instance.data_dir);
    for (key, value) in &instance.state.environment {
        command.env(key, value);
    }
    for port in &instance.recipe.runtime.ports {
        let value = instance
            .state
            .ports
            .get(&port.name)
            .ok_or_else(|| format!("Missing allocated port {}", port.name))?;
        command.env(&port.environment, value.to_string());
    }
    for secret in &instance.recipe.secrets {
        command.env(
            &secret.key,
            secret_store::get(&instance.recipe.id, &secret.key)?,
        );
    }
    command
        .output()
        .map_err(|error| format!("Cannot run Docker Compose: {error}"))
}

fn checked_compose(instance: &Instance, arguments: &[&str]) -> Result<String, String> {
    let output = compose_output(instance, arguments)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if message.is_empty() {
            "Docker Compose failed without an error message".into()
        } else {
            message
        })
    }
}

fn is_running(instance: &Instance) -> bool {
    compose_output(
        instance,
        &["ps", "--services", "--filter", "status=running"],
    )
    .map(|output| output.status.success() && !output.stdout.is_empty())
    .unwrap_or(false)
}

fn health_socket(url: &str) -> Option<SocketAddr> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let port = parsed.port_or_known_default()?;
    (host, port).to_socket_addrs().ok()?.next()
}

fn is_healthy(instance: &Instance) -> bool {
    recipe_url(&instance.recipe, &instance.state)
        .ok()
        .as_deref()
        .and_then(health_socket)
        .and_then(|address| TcpStream::connect_timeout(&address, Duration::from_millis(350)).ok())
        .is_some()
}

fn current_status(instance: &Instance) -> String {
    if !is_running(instance) {
        "stopped".into()
    } else if is_healthy(instance) {
        "ready".into()
    } else {
        "starting".into()
    }
}

pub fn list_apps(engine: &Engine) -> Result<Vec<AppSummary>, String> {
    let root = apps_root(engine)?;
    let mut apps = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| format!("Cannot list apps: {error}"))? {
        let entry = entry.map_err(|error| format!("Cannot read app entry: {error}"))?;
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if let Ok(instance) = load_instance(engine, &id) {
            apps.push(AppSummary {
                id: instance.recipe.id.clone(),
                name: instance.recipe.name.clone(),
                version: instance.state.installed_version.clone(),
                description: instance.recipe.description.clone(),
                category: instance.recipe.category.clone(),
                status: current_status(&instance),
                automatic_updates: instance.state.automatic_updates,
                url: recipe_url(&instance.recipe, &instance.state)?,
                last_update_check: instance.state.last_update_check,
            });
        }
    }
    apps.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(apps)
}

fn update_due(instance: &Instance) -> bool {
    let interval = instance
        .recipe
        .updates
        .interval_hours
        .saturating_mul(60 * 60);
    instance
        .state
        .last_update_check
        .map(|last| now().saturating_sub(last) >= interval)
        .unwrap_or(true)
}

fn pull_update(engine: &Engine, id: &str, force: bool) -> Result<String, String> {
    let mut instance = load_instance(engine, id)?;
    if !force && (!instance.state.automatic_updates || !update_due(&instance)) {
        return Ok("Already up to date".into());
    }
    managed_runtime::ensure_running(engine)?;
    checked_compose(&instance, &["pull", "--quiet"])?;
    instance.state.last_update_check = Some(now());
    save_state(engine, id, &instance.state)?;
    Ok("Latest images downloaded".into())
}

pub fn start(engine: &Engine, id: &str) -> Result<ActionResult, String> {
    managed_runtime::ensure_running(engine)?;
    let mut instance = load_instance(engine, id)?;
    if !is_running(&instance) {
        let mut changed = false;
        let mut seen = std::collections::HashSet::new();
        for port in &instance.recipe.runtime.ports {
            let allocated = instance.state.ports.get(&port.name).copied().unwrap_or(0);
            if allocated == 0 || seen.contains(&allocated) || !port_available(allocated) {
                let replacement = allocate_unique_port(&seen)?;
                instance.state.ports.insert(port.name.clone(), replacement);
                seen.insert(replacement);
                changed = true;
            } else {
                seen.insert(allocated);
            }
        }
        if changed {
            save_state(engine, id, &instance.state)?;
        }
    }
    let update_message = if instance.state.automatic_updates && update_due(&instance) {
        match pull_update(engine, id, false) {
            Ok(message) => message,
            Err(error) => format!("Update check skipped: {error}"),
        }
    } else {
        "Update check not due".into()
    };
    let instance = load_instance(engine, id)?;
    checked_compose(&instance, &["up", "-d", "--remove-orphans"])?;
    Ok(ActionResult {
        id: id.into(),
        status: current_status(&instance),
        message: format!("{} started. {update_message}.", instance.recipe.name),
    })
}

pub fn stop(engine: &Engine, id: &str) -> Result<ActionResult, String> {
    let instance = load_instance(engine, id)?;
    if !managed_runtime::status(engine)?.running {
        return Ok(ActionResult {
            id: id.into(),
            status: "stopped".into(),
            message: format!("{} is already stopped.", instance.recipe.name),
        });
    }
    checked_compose(&instance, &["stop"])?;
    Ok(ActionResult {
        id: id.into(),
        status: "stopped".into(),
        message: format!("{} stopped. Your data is preserved.", instance.recipe.name),
    })
}

pub fn update(engine: &Engine, id: &str) -> Result<ActionResult, String> {
    managed_runtime::ensure_running(engine)?;
    let was_running = is_running(&load_instance(engine, id)?);
    let message = pull_update(engine, id, true)?;
    let instance = load_instance(engine, id)?;
    if was_running {
        checked_compose(&instance, &["up", "-d", "--remove-orphans"])?;
    }
    Ok(ActionResult {
        id: id.into(),
        status: current_status(&instance),
        message: format!("{}. {} is current.", message, instance.recipe.name),
    })
}

pub fn set_automatic_updates(
    engine: &Engine,
    id: &str,
    enabled: bool,
) -> Result<ActionResult, String> {
    let mut instance = load_instance(engine, id)?;
    instance.state.automatic_updates = enabled;
    save_state(engine, id, &instance.state)?;
    Ok(ActionResult {
        id: id.into(),
        status: current_status(&instance),
        message: if enabled {
            "Automatic updates enabled".into()
        } else {
            "Automatic updates disabled".into()
        },
    })
}

pub fn logs(engine: &Engine, id: &str, lines: u32) -> Result<String, String> {
    let instance = load_instance(engine, id)?;
    let count = lines.clamp(20, 1000).to_string();
    checked_compose(&instance, &["logs", "--no-color", "--tail", &count])
}

pub fn app_url(engine: &Engine, id: &str) -> Result<String, String> {
    let instance = load_instance(engine, id)?;
    if !is_healthy(&instance) {
        return Err(format!("{} is not ready yet", instance.recipe.name));
    }
    recipe_url(&instance.recipe, &instance.state)
}

pub fn uninstall(engine: &Engine, id: &str, delete_data: bool) -> Result<ActionResult, String> {
    let instance = load_instance(engine, id)?;
    let down_arguments = if delete_data {
        vec!["down", "--remove-orphans", "--volumes"]
    } else {
        vec!["down", "--remove-orphans"]
    };
    let _ = checked_compose(&instance, &down_arguments);
    remove_launcher(&instance.recipe)?;
    let root = instance_root(engine, id)?;
    if delete_data {
        for key in &instance.state.secret_keys {
            secret_store::remove(&instance.recipe.id, key)?;
        }
        fs::remove_dir_all(&root).map_err(|error| format!("Cannot remove app data: {error}"))?;
    } else {
        fs::remove_dir_all(root.join("definition"))
            .map_err(|error| format!("Cannot remove app definition: {error}"))?;
        let _ = fs::remove_file(root.join("state.json"));
    }
    Ok(ActionResult {
        id: id.into(),
        status: "not-installed".into(),
        message: if delete_data {
            format!("{} and all of its data were removed", instance.recipe.name)
        } else {
            format!("{} was removed; its data was kept", instance.recipe.name)
        },
    })
}

pub fn automatic_updates(engine: &Engine) -> Result<Vec<ActionResult>, String> {
    if !managed_runtime::status(engine)?.installed {
        return Ok(Vec::new());
    }
    let ids = list_apps(engine)?
        .into_iter()
        .filter(|item| item.automatic_updates)
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let mut results = Vec::new();
    for id in ids {
        let instance = load_instance(engine, &id)?;
        if update_due(&instance) {
            match update(engine, &id) {
                Ok(result) => results.push(result),
                Err(error) => results.push(ActionResult {
                    id,
                    status: current_status(&instance),
                    message: format!("Automatic update failed: {error}"),
                }),
            }
        }
    }
    Ok(results)
}

pub fn system_status(engine: &Engine) -> Result<SystemStatus, String> {
    let runtime = managed_runtime::status(engine)?;
    Ok(SystemStatus {
        engine_available: runtime.running,
        engine_name: "Packager Runtime".into(),
        engine_version: runtime.version.clone(),
        app_data_dir: apps_root(engine)?.to_string_lossy().to_string(),
        runtime,
    })
}

pub(crate) fn copy_package_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fn copy(
        source: &Path,
        destination: &Path,
        file_count: &mut usize,
        total_bytes: &mut u64,
    ) -> Result<(), String> {
        fs::create_dir_all(destination)
            .map_err(|error| format!("Cannot create {}: {error}", destination.display()))?;
        for entry in fs::read_dir(source)
            .map_err(|error| format!("Cannot read {}: {error}", source.display()))?
        {
            let entry = entry.map_err(|error| format!("Cannot read package entry: {error}"))?;
            let name = entry.file_name();
            let name_text = name.to_string_lossy();
            if matches!(
                name_text.as_ref(),
                ".git" | "node_modules" | "target" | ".DS_Store"
            ) {
                continue;
            }
            let source_path = entry.path();
            let metadata = fs::symlink_metadata(&source_path)
                .map_err(|error| format!("Cannot inspect {}: {error}", source_path.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "Package source contains a symbolic link, which must be replaced before import: {}",
                    source_path.display()
                ));
            }
            let destination_path = destination.join(name);
            if metadata.is_dir() {
                copy(&source_path, &destination_path, file_count, total_bytes)?;
            } else if metadata.is_file() {
                *file_count += 1;
                *total_bytes = total_bytes.saturating_add(metadata.len());
                if *file_count > 25_000 || *total_bytes > 1024 * 1024 * 1024 {
                    return Err(
                        "Package source exceeds the 25,000 file or 1 GB safety limit".into(),
                    );
                }
                fs::copy(&source_path, &destination_path).map_err(|error| {
                    format!(
                        "Cannot copy {} to {}: {error}",
                        source_path.display(),
                        destination_path.display()
                    )
                })?;
            }
        }
        Ok(())
    }

    let source = fs::canonicalize(source)
        .map_err(|error| format!("Cannot access package source: {error}"))?;
    if !source.is_dir() {
        return Err("Package source must be a directory".into());
    }
    if let Ok(destination) = fs::canonicalize(destination) {
        if destination.starts_with(&source) {
            return Err("Package output cannot be placed inside its own source directory".into());
        }
    }
    let mut file_count = 0;
    let mut total_bytes = 0;
    copy(&source, destination, &mut file_count, &mut total_bytes)
}

pub fn import_package(engine: &Engine, source_dir: &str) -> Result<ActionResult, String> {
    let source = PathBuf::from(source_dir);
    let recipe_source = fs::read_to_string(source.join("packager.yml"))
        .map_err(|error| format!("Cannot read packager.yml: {error}"))?;
    let recipe = parse_recipe(&recipe_source)?;
    let compose_source = source.join(&recipe.runtime.compose_file);
    if !compose_source.is_file() {
        return Err(format!("Missing {}", recipe.runtime.compose_file));
    }
    validate_compose_security(&compose_source)?;
    let root = instance_root(engine, &recipe.id)?;
    if root.join("state.json").exists() {
        return Err(format!("{} is already installed", recipe.name));
    }
    let definition = root.join("definition");
    let staging = root.join(format!("definition-staging-{}", Uuid::new_v4()));
    fs::create_dir_all(&root)
        .and_then(|_| fs::create_dir_all(root.join("data")))
        .map_err(|error| format!("Cannot create package directories: {error}"))?;
    let copy_result = copy_package_tree(&source, &staging);
    if let Err(error) = copy_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if definition.exists() {
        fs::remove_dir_all(&definition)
            .map_err(|error| format!("Cannot replace package definition: {error}"))?;
    }
    fs::rename(&staging, &definition)
        .map_err(|error| format!("Cannot install package definition: {error}"))?;
    let state = prepare_state(&recipe, None)?;
    write_json(&root.join("state.json"), &state)?;
    if engine.launchers_enabled() {
        install_launcher(&recipe, &definition, engine.launcher_icon())?;
    }
    Ok(ActionResult {
        id: recipe.id,
        status: "stopped".into(),
        message: format!("{} imported successfully", recipe.name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_recipe_is_valid() {
        let recipe = parse_recipe(OPEN_NOTEBOOK_RECIPE).expect("recipe should parse");
        assert_eq!(recipe.id, "open-notebook");
        assert_eq!(recipe.runtime.kind, "compose");
        assert_eq!(recipe.ui.port.as_deref(), Some("web"));
        assert_eq!(recipe.runtime.ports.len(), 2);
        assert_eq!(recipe.secrets[0].key, "OPEN_NOTEBOOK_ENCRYPTION_KEY");
        assert!(OPEN_NOTEBOOK_COMPOSE.contains(
            "API_URL: http://127.0.0.1:${PACKAGER_API_PORT:?PACKAGER_API_PORT is required}"
        ));
        let compose = std::env::temp_dir().join(format!("packager-compose-{}.yml", Uuid::new_v4()));
        fs::write(&compose, OPEN_NOTEBOOK_COMPOSE).expect("compose should be writable");
        validate_compose_security(&compose).expect("bundled compose should be safe");
        fs::remove_file(compose).expect("compose test file should be removable");
    }

    #[test]
    fn recipe_rejects_path_escape() {
        let mut recipe = parse_recipe(OPEN_NOTEBOOK_RECIPE).expect("recipe should parse");
        recipe.runtime.compose_file = "../compose.yml".into();
        assert!(validate_recipe(&recipe).is_err());
    }

    #[test]
    fn recipe_rejects_unknown_fields() {
        let source = format!("{OPEN_NOTEBOOK_RECIPE}\nunreviewed_command: rm -rf /\n");
        assert!(parse_recipe(&source).is_err());
    }

    #[test]
    fn compose_security_blocks_privileged_and_public_ports() {
        for source in [
            "services:\n  app:\n    image: example/app\n    privileged: true\n",
            "services:\n  app:\n    image: example/app\n    ports:\n      - '3000:3000'\n",
        ] {
            let compose =
                std::env::temp_dir().join(format!("packager-unsafe-{}.yml", Uuid::new_v4()));
            fs::write(&compose, source).expect("compose should be writable");
            assert!(validate_compose_security(&compose).is_err());
            fs::remove_file(compose).expect("compose test file should be removable");
        }
    }

    #[test]
    fn healthcheck_resolves_localhost() {
        assert!(health_socket("http://127.0.0.1:8502").is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launcher_bundle_contains_a_deep_link() {
        let recipe = parse_recipe(OPEN_NOTEBOOK_RECIPE).expect("recipe should parse");
        let bundle =
            std::env::temp_dir().join(format!("packager-launcher-test-{}.app", Uuid::new_v4()));
        write_launcher_bundle(&recipe, &bundle, b"test-icon").expect("launcher should be written");
        let script = fs::read_to_string(bundle.join("Contents/MacOS/launch"))
            .expect("launcher executable should be readable");
        assert!(script.contains("open -b dev.packager.desktop"));
        assert!(script.contains("packager://open/open-notebook"));
        assert!(bundle.join("Contents/Info.plist").is_file());
        assert!(bundle.join("Contents/Resources/AppIcon.icns").is_file());
        fs::remove_dir_all(bundle).expect("launcher test bundle should be removable");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cloned_launcher_has_its_own_native_identity() {
        let recipe = parse_recipe(OPEN_NOTEBOOK_RECIPE).expect("recipe should parse");
        let root = std::env::temp_dir().join(format!("packager-clone-test-{}", Uuid::new_v4()));
        let source = root.join("Packager.app");
        let bundle = root.join("Open Notebook.app");
        write_launcher_bundle(&recipe, &source, b"source-icon")
            .expect("source app should be written");
        clone_launcher_bundle(&recipe, &source, &bundle, b"open-notebook-icon")
            .expect("native launcher should be cloned");

        assert_eq!(
            fs::read_to_string(bundle.join("Contents/Resources/packager-launcher-id"))
                .expect("launcher marker should be readable"),
            "open-notebook"
        );
        assert_eq!(
            fs::read(bundle.join("Contents/Resources/AppIcon.icns"))
                .expect("launcher icon should be readable"),
            b"open-notebook-icon"
        );
        assert!(bundle.join("Contents/MacOS/launch").is_file());
        let identifier = Command::new("/usr/bin/plutil")
            .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
            .arg(bundle.join("Contents/Info.plist"))
            .output()
            .expect("launcher metadata should be readable");
        assert_eq!(
            String::from_utf8_lossy(&identifier.stdout).trim(),
            "dev.packager.launcher.open.notebook"
        );
        fs::remove_dir_all(root).expect("launcher test directory should be removable");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn portable_png_becomes_a_native_macos_icon() {
        let root = std::env::temp_dir().join(format!("packager-icon-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("icon test directory should be created");
        let source = root.join("icon.png");
        let destination = root.join("icon.icns");
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1024, 1024)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("test icon should encode");
        fs::write(&source, png.into_inner()).expect("test icon should be written");
        create_macos_icon(&source, &destination).expect("native icon should be created");
        assert!(fs::metadata(&destination).is_ok_and(|metadata| metadata.len() > 0));
        fs::remove_dir_all(root).expect("icon test directory should be removable");
    }
}
