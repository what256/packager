use crate::{
    icon,
    model::{
        ActionResult, BuilderAnalysis, BuilderRequest, PackageRecipe, PortRecipe, Requirements,
        RuntimeRecipe, SecretRecipe, ServiceAnalysis, UiRecipe, UpdateRecipe,
    },
    runtime, Engine,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use flate2::read::GzDecoder;
use serde_yaml::{Mapping, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use url::Url;
use uuid::Uuid;

const MAX_COMPOSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ICON_SCAN_ENTRIES: usize = 5000;

struct SourceBundle {
    root: Option<PathBuf>,
    compose: Value,
    source_label: String,
    temporary_root: Option<PathBuf>,
}

impl Drop for SourceBundle {
    fn drop(&mut self) {
        if let Some(root) = &self.temporary_root {
            let _ = fs::remove_dir_all(root);
        }
    }
}

fn yaml_key(key: &str) -> Value {
    Value::String(key.into())
}

fn safe_image_name(image: &str) -> bool {
    !image.is_empty()
        && !image.starts_with('-')
        && image.len() <= 300
        && image.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '-' | '/' | ':' | '@')
        })
}

fn find_compose(root: &Path) -> Result<PathBuf, String> {
    fn find(root: &Path, depth: usize) -> Option<PathBuf> {
        for name in [
            "compose.yml",
            "compose.yaml",
            "docker-compose.yml",
            "docker-compose.yaml",
        ] {
            let candidate = root.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if depth == 0 {
            return None;
        }
        let mut directories = fs::read_dir(root)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.')
            })
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        directories.sort();
        for directory in directories {
            if let Some(found) = find(&directory, depth - 1) {
                return Some(found);
            }
        }
        None
    }
    if let Some(found) = find(root, 3) {
        return Ok(found);
    }
    Err(format!(
        "No compose.yml, compose.yaml, docker-compose.yml, or docker-compose.yaml found in {}",
        root.display()
    ))
}

fn read_compose(path: &Path) -> Result<Value, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?;
    if metadata.len() > MAX_COMPOSE_BYTES {
        return Err("Compose file is larger than the 2 MB safety limit".into());
    }
    let source = fs::read_to_string(path)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let value: Value = serde_yaml::from_str(&source)
        .map_err(|error| format!("Cannot parse {}: {error}", path.display()))?;
    if value
        .as_mapping()
        .and_then(|mapping| mapping.get(yaml_key("services")))
        .and_then(Value::as_mapping)
        .map(Mapping::is_empty)
        .unwrap_or(true)
    {
        return Err("Compose file must contain at least one service".into());
    }
    Ok(value)
}

fn image_compose(image: &str) -> Result<Value, String> {
    if !safe_image_name(image) {
        return Err("Image must be a valid registry/image reference".into());
    }
    let source = format!("services:\n  app:\n    image: {image}\n    restart: unless-stopped\n");
    serde_yaml::from_str(&source).map_err(|error| format!("Cannot create image package: {error}"))
}

fn github_parts(source: &str) -> Result<(String, String, String), String> {
    let url = Url::parse(source).map_err(|_| "Enter a full https://github.com URL")?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err("Only public https://github.com repository URLs are supported".into());
    }
    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if segments.len() < 2 {
        return Err("GitHub URL must include an owner and repository".into());
    }
    let owner = segments[0];
    let repository = segments[1].trim_end_matches(".git");
    let branch = if segments.get(2) == Some(&"tree") {
        segments.get(3).copied().unwrap_or("main")
    } else {
        "main"
    };
    let safe = |value: &str| {
        !value.is_empty()
            && value.len() <= 100
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
    };
    if !safe(owner) || !safe(repository) || !safe(branch) {
        return Err("GitHub owner, repository, or branch contains unsupported characters".into());
    }
    Ok((owner.into(), repository.into(), branch.into()))
}

fn download_github(engine: &Engine, source: &str) -> Result<PathBuf, String> {
    let (owner, repository, branch) = github_parts(source)?;
    let root = engine
        .cache_dir()
        .join("builder")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&root)
        .map_err(|error| format!("Cannot create GitHub source cache: {error}"))?;
    let archive_url =
        format!("https://codeload.github.com/{owner}/{repository}/tar.gz/refs/heads/{branch}");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(5 * 60))
        .user_agent("Packager package builder")
        .build()
        .map_err(|error| format!("Cannot prepare GitHub download: {error}"))?;
    let response = client
        .get(&archive_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Cannot download GitHub repository: {error}"))?;
    let mut bytes = Vec::new();
    response
        .take(500 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read GitHub repository: {error}"))?;
    if bytes.len() > 500 * 1024 * 1024 {
        return Err("GitHub repository archive exceeds the 500 MB safety limit".into());
    }
    let mut archive = tar::Archive::new(GzDecoder::new(bytes.as_slice()));
    for entry in archive
        .entries()
        .map_err(|error| format!("Cannot inspect GitHub archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("Invalid GitHub archive: {error}"))?;
        if !entry
            .unpack_in(&root)
            .map_err(|error| format!("Cannot unpack GitHub repository: {error}"))?
        {
            return Err("GitHub archive contains an unsafe path".into());
        }
    }
    let extracted = fs::read_dir(&root)
        .map_err(|error| format!("Cannot inspect GitHub repository: {error}"))?
        .filter_map(Result::ok)
        .find(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .ok_or("GitHub repository archive was empty")?;
    Ok(extracted)
}

fn source_bundle(engine: &Engine, kind: &str, source: &str) -> Result<SourceBundle, String> {
    match kind {
        "image" => Ok(SourceBundle {
            root: None,
            compose: image_compose(source.trim())?,
            source_label: source.trim().into(),
            temporary_root: None,
        }),
        "compose" => {
            let supplied = PathBuf::from(source.trim());
            let canonical = fs::canonicalize(&supplied)
                .map_err(|error| format!("Cannot access {}: {error}", supplied.display()))?;
            let (root, compose_path) = if canonical.is_dir() {
                let compose = find_compose(&canonical)?;
                (canonical, compose)
            } else {
                let parent = canonical
                    .parent()
                    .ok_or("Compose file has no parent directory")?
                    .to_path_buf();
                (parent, canonical)
            };
            Ok(SourceBundle {
                compose: read_compose(&compose_path)?,
                source_label: compose_path.to_string_lossy().to_string(),
                root: Some(root),
                temporary_root: None,
            })
        }
        "github" => {
            let root = download_github(engine, source.trim())?;
            let compose_path = find_compose(&root)?;
            let package_root = compose_path
                .parent()
                .ok_or("Compose file has no parent directory")?
                .to_path_buf();
            let temporary_root = root.parent().map(Path::to_path_buf);
            Ok(SourceBundle {
                compose: read_compose(&compose_path)?,
                source_label: source.trim().into(),
                root: Some(package_root),
                temporary_root,
            })
        }
        _ => Err("Source kind must be compose, image, or github".into()),
    }
}

fn container_port(value: &Value) -> Option<u16> {
    if let Some(number) = value.as_u64() {
        return u16::try_from(number).ok().filter(|port| *port > 0);
    }
    if let Some(text) = value.as_str() {
        let last = text
            .rsplit(':')
            .next()?
            .split('/')
            .next()?
            .split('-')
            .next()?;
        return last.parse::<u16>().ok().filter(|port| *port > 0);
    }
    value
        .as_mapping()?
        .get(yaml_key("target"))
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Sequence(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::Mapping(values)) => values
            .keys()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn analyze_bundle(bundle: &SourceBundle) -> Result<BuilderAnalysis, String> {
    let services = bundle
        .compose
        .as_mapping()
        .and_then(|mapping| mapping.get(yaml_key("services")))
        .and_then(Value::as_mapping)
        .ok_or("Compose services must be a mapping")?;
    let mut result = Vec::new();
    let mut candidate_ports = BTreeSet::new();
    let mut warnings = Vec::new();
    for (name, raw_service) in services {
        let name = name
            .as_str()
            .ok_or("Compose service names must be strings")?;
        let service = raw_service
            .as_mapping()
            .ok_or_else(|| format!("Service {name} must be an object"))?;
        let ports = service
            .get(yaml_key("ports"))
            .and_then(Value::as_sequence)
            .map(|ports| ports.iter().filter_map(container_port).collect::<Vec<_>>())
            .unwrap_or_default();
        candidate_ports.extend(ports.iter().copied());
        let volumes = string_list(service.get(yaml_key("volumes")));
        if service.contains_key(yaml_key("build")) {
            warnings.push(format!(
                "{name} builds locally; Packager will copy its build context into the package"
            ));
        }
        if volumes.iter().any(|volume| volume.starts_with('/')) {
            warnings.push(format!(
                "{name} has an absolute bind mount; replace it with ${{PACKAGER_DATA_DIR}} before sharing"
            ));
        }
        result.push(ServiceAnalysis {
            name: name.into(),
            image: service
                .get(yaml_key("image"))
                .and_then(Value::as_str)
                .map(str::to_string),
            ports,
            volumes,
            environment: string_list(service.get(yaml_key("environment"))),
        });
    }
    if candidate_ports.is_empty() {
        warnings.push("No published port was detected; enter the container's web port".into());
    }
    let detected_name = result
        .iter()
        .find_map(|service| service.image.as_deref())
        .and_then(|image| image.rsplit('/').next())
        .and_then(|image| image.split(':').next())
        .unwrap_or("local-app")
        .replace(['_', '.'], " ");
    let icon = bundle.root.as_deref().and_then(find_source_icon);
    let detected_icon = icon.as_ref().map(|path| {
        bundle
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    });
    let preview_icon = icon
        .as_ref()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| !extension.eq_ignore_ascii_case("icns"))
        })
        .cloned()
        .or_else(|| bundle.root.as_deref().and_then(find_portable_source_icon));
    let icon_preview_data_url = preview_icon
        .as_deref()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| icon::normalize_to_png(&bytes).ok())
        .map(|png| format!("data:image/png;base64,{}", BASE64.encode(png)));
    Ok(BuilderAnalysis {
        source: bundle.source_label.clone(),
        detected_name,
        services: result,
        candidate_ports: candidate_ports.into_iter().collect(),
        warnings,
        detected_icon,
        icon_preview_data_url,
    })
}

fn icon_name_score(path: &Path) -> Option<u32> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let extension_score = match extension.as_str() {
        "icns" | "ico" => 0,
        "png" => 1,
        "jpg" | "jpeg" | "webp" => 2,
        _ => return None,
    };
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let name_score = if matches!(stem.as_str(), "appicon" | "app-icon" | "icon") {
        0
    } else if stem.contains("appicon") || stem.contains("app-icon") {
        5
    } else if stem == "logo" || stem.ends_with("-logo") || stem.ends_with("_logo") {
        10
    } else if stem.contains("logo") {
        15
    } else if stem == "favicon" || stem.starts_with("favicon-") {
        20
    } else {
        return None;
    };
    Some(name_score + extension_score)
}

fn source_icon_candidates(root: &Path) -> Vec<PathBuf> {
    fn visit(
        root: &Path,
        directory: &Path,
        depth: usize,
        seen: &mut usize,
        matches: &mut Vec<(u32, PathBuf)>,
    ) {
        if depth > 4 || *seen >= MAX_ICON_SCAN_ENTRIES {
            return;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            *seen += 1;
            if *seen > MAX_ICON_SCAN_ENTRIES {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !name.starts_with('.')
                    && !matches!(name.as_str(), "node_modules" | "target" | "vendor")
                {
                    visit(root, &path, depth + 1, seen, matches);
                }
            } else if file_type.is_file() {
                if let Some(score) = icon_name_score(&path) {
                    let relative_depth = path
                        .strip_prefix(root)
                        .ok()
                        .map(|relative| relative.components().count() as u32)
                        .unwrap_or(10);
                    matches.push((score + relative_depth, path));
                }
            }
        }
    }

    let mut matches = Vec::new();
    let mut seen = 0;
    visit(root, root, 0, &mut seen, &mut matches);
    matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    matches.into_iter().map(|(_, path)| path).collect()
}

fn find_source_icon(root: &Path) -> Option<PathBuf> {
    source_icon_candidates(root).into_iter().next()
}

fn find_portable_source_icon(root: &Path) -> Option<PathBuf> {
    source_icon_candidates(root).into_iter().find(|path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| !extension.eq_ignore_ascii_case("icns"))
    })
}

fn custom_icon_data(data: &str) -> Result<Vec<u8>, String> {
    icon::decode_data_url(data)
}

fn write_package_icon(
    bundle: &SourceBundle,
    request: &BuilderRequest,
    export_root: &Path,
) -> Result<(), String> {
    if let Some(data) = request.icon_data.as_deref() {
        return fs::write(export_root.join("icon.png"), custom_icon_data(data)?)
            .map_err(|error| format!("Cannot write custom app icon: {error}"));
    }
    let Some(source) = bundle.root.as_deref().and_then(find_source_icon) else {
        return Ok(());
    };
    let bytes =
        fs::read(&source).map_err(|error| format!("Cannot read detected app icon: {error}"))?;
    match source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("icns") => {
            fs::write(export_root.join("icon.icns"), bytes)
                .map_err(|error| format!("Cannot copy detected macOS icon: {error}"))?;
            if let Some(portable) = bundle.root.as_deref().and_then(find_portable_source_icon) {
                if let Ok(png) = fs::read(portable)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| icon::normalize_to_png(&bytes))
                {
                    fs::write(export_root.join("icon.png"), png)
                        .map_err(|error| format!("Cannot prepare detected app icon: {error}"))?;
                }
            }
        }
        Some("ico") => {
            fs::write(export_root.join("icon.ico"), &bytes)
                .map_err(|error| format!("Cannot copy detected Windows icon: {error}"))?;
            if let Ok(png) = icon::normalize_to_png(&bytes) {
                fs::write(export_root.join("icon.png"), png)
                    .map_err(|error| format!("Cannot prepare detected app icon: {error}"))?;
            }
        }
        _ => fs::write(
            export_root.join("icon.png"),
            icon::normalize_to_png(&bytes)?,
        )
        .map_err(|error| format!("Cannot prepare detected app icon: {error}"))?,
    }
    Ok(())
}

pub fn analyze(engine: &Engine, kind: &str, source: &str) -> Result<BuilderAnalysis, String> {
    analyze_bundle(&source_bundle(engine, kind, source)?)
}

fn services_mut(compose: &mut Value) -> Result<&mut Mapping, String> {
    compose
        .as_mapping_mut()
        .and_then(|mapping| mapping.get_mut(yaml_key("services")))
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| "Compose services must be a mapping".into())
}

fn rewrite_ports(compose: &mut Value, web_port: u16) -> Result<Vec<PortRecipe>, String> {
    let services = services_mut(compose)?;
    let mut declared = Vec::<PortRecipe>::new();
    let mut found_web = false;
    for raw_service in services.values_mut() {
        let Some(service) = raw_service.as_mapping_mut() else {
            continue;
        };
        let Some(ports) = service
            .get_mut(yaml_key("ports"))
            .and_then(Value::as_sequence_mut)
        else {
            continue;
        };
        for raw_port in ports.iter_mut() {
            let Some(target) = container_port(raw_port) else {
                continue;
            };
            let is_web = target == web_port && !found_web;
            let name = if is_web {
                found_web = true;
                "web".into()
            } else {
                format!("port-{target}")
            };
            if declared.iter().any(|port| port.name == name) {
                continue;
            }
            let environment = if is_web {
                "PACKAGER_WEB_PORT".into()
            } else {
                format!("PACKAGER_PORT_{target}")
            };
            *raw_port = Value::String(format!(
                "127.0.0.1:${{{environment}:?{environment} is required}}:{target}"
            ));
            declared.push(PortRecipe {
                name,
                container_port: target,
                environment,
            });
        }
    }
    if !found_web {
        let first = services
            .values_mut()
            .next()
            .and_then(Value::as_mapping_mut)
            .ok_or("Compose has no usable service")?;
        let ports = first
            .entry(yaml_key("ports"))
            .or_insert_with(|| Value::Sequence(Vec::new()))
            .as_sequence_mut()
            .ok_or("First service ports must be a list")?;
        ports.push(Value::String(format!(
            "127.0.0.1:${{PACKAGER_WEB_PORT:?PACKAGER_WEB_PORT is required}}:{web_port}"
        )));
        declared.push(PortRecipe {
            name: "web".into(),
            container_port: web_port,
            environment: "PACKAGER_WEB_PORT".into(),
        });
    }
    Ok(declared)
}

fn inject_secrets(compose: &mut Value, keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Ok(());
    }
    let first = services_mut(compose)?
        .values_mut()
        .next()
        .and_then(Value::as_mapping_mut)
        .ok_or("Compose has no usable service")?;
    let environment = first
        .entry(yaml_key("environment"))
        .or_insert_with(|| Value::Mapping(Mapping::new()))
        .as_mapping_mut()
        .ok_or("First service environment must use key/value syntax to add secrets")?;
    for key in keys {
        environment
            .entry(yaml_key(key))
            .or_insert_with(|| Value::String(format!("${{{key}:?{key} is required}}")));
    }
    Ok(())
}

fn slug_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn secret_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

pub fn build(engine: &Engine, request: BuilderRequest) -> Result<ActionResult, String> {
    if !slug_valid(&request.id) {
        return Err("Package id must use lowercase letters, numbers, and hyphens".into());
    }
    if request.name.trim().is_empty() || request.container_port == 0 {
        return Err("Package name and web container port are required".into());
    }
    let mut secret_keys = request
        .secret_keys
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if secret_keys.iter().any(|key| !secret_valid(key)) {
        return Err("Secret keys must use uppercase letters, numbers, and underscores".into());
    }
    let bundle = source_bundle(engine, &request.source_kind, &request.source)?;
    let mut compose = bundle.compose.clone();
    let ports = rewrite_ports(&mut compose, request.container_port)?;
    inject_secrets(&mut compose, &secret_keys)?;

    let export_root = engine.data_dir().join("created-packages").join(&request.id);
    if export_root.exists() {
        return Err(format!(
            "A generated package already exists at {}. Rename the package or remove that folder first.",
            export_root.display()
        ));
    }
    fs::create_dir_all(&export_root)
        .map_err(|error| format!("Cannot create package output: {error}"))?;
    let result = (|| {
        if let Some(root) = &bundle.root {
            runtime::copy_package_tree(root, &export_root)?;
        }
        write_package_icon(&bundle, &request, &export_root)?;
        let recipe = PackageRecipe {
            schema_version: 1,
            id: request.id.clone(),
            name: request.name.trim().into(),
            version: "1.0.0".into(),
            description: if request.description.trim().is_empty() {
                format!("{} packaged for local desktop use.", request.name.trim())
            } else {
                request.description.trim().into()
            },
            category: "Local app".into(),
            homepage: if request.homepage.trim().is_empty() {
                "https://github.com".into()
            } else {
                request.homepage.trim().into()
            },
            license: "Custom".into(),
            runtime: RuntimeRecipe {
                kind: "compose".into(),
                compose_file: "compose.yml".into(),
                project_name: format!("packager-{}", request.id),
                ports,
            },
            ui: UiRecipe {
                url: None,
                port: Some("web".into()),
                path: "/".into(),
                width: 1280.0,
                height: 820.0,
            },
            requirements: Requirements::default(),
            updates: UpdateRecipe::default(),
            secrets: secret_keys
                .drain(..)
                .map(|key| SecretRecipe {
                    key,
                    generate: "uuid".into(),
                })
                .collect(),
        };
        runtime::validate_recipe(&recipe)?;
        fs::write(
            export_root.join("packager.yml"),
            serde_yaml::to_string(&recipe)
                .map_err(|error| format!("Cannot generate packager.yml: {error}"))?,
        )
        .and_then(|_| {
            fs::write(
                export_root.join("compose.yml"),
                serde_yaml::to_string(&compose).map_err(std::io::Error::other)?,
            )
        })
        .map_err(|error| format!("Cannot write generated package: {error}"))?;
        runtime::import_package(engine, export_root.to_string_lossy().as_ref())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&export_root);
    }
    let mut action = result?;
    action.message = format!(
        "{} was generated, validated, and installed. Shareable files: {}",
        request.name.trim(),
        export_root.display()
    );
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_png() -> Vec<u8> {
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(48, 32)
            .write_to(&mut output, image::ImageFormat::Png)
            .expect("test icon should encode");
        output.into_inner()
    }

    #[test]
    fn parses_common_compose_port_forms() {
        for (source, expected) in [
            ("3000", 3000),
            ("127.0.0.1:8080:3000", 3000),
            ("8080:3000/tcp", 3000),
        ] {
            assert_eq!(
                container_port(&Value::String(source.into())),
                Some(expected)
            );
        }
    }

    #[test]
    fn validates_registry_image_names() {
        assert!(safe_image_name("ghcr.io/acme/app:1.2"));
        assert!(!safe_image_name("--help"));
        assert!(!safe_image_name("image;rm"));
    }

    #[test]
    fn image_builder_adds_dynamic_loopback_port_and_secret() {
        let mut compose =
            image_compose("ghcr.io/acme/app:1.2").expect("image compose should parse");
        let ports = rewrite_ports(&mut compose, 3000).expect("port should be generated");
        inject_secrets(&mut compose, &["API_KEY".into()]).expect("secret should be generated");
        let rendered = serde_yaml::to_string(&compose).expect("compose should render");
        assert_eq!(ports[0].name, "web");
        assert!(rendered.contains("127.0.0.1:${PACKAGER_WEB_PORT"));
        assert!(rendered.contains("API_KEY: ${API_KEY:?API_KEY is required}"));
    }

    #[test]
    fn github_source_is_restricted_to_public_repository_urls() {
        assert!(github_parts("https://github.com/acme/example").is_ok());
        assert!(github_parts("https://gitlab.com/acme/example").is_err());
        assert!(github_parts("file:///tmp/example").is_err());
    }

    #[test]
    fn analysis_finds_and_previews_an_original_app_icon() {
        let root = std::env::temp_dir().join(format!("packager-icon-source-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("public")).expect("source should be created");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: example/my-app\n    ports:\n      - '3000:3000'\n",
        )
        .expect("compose should be written");
        fs::write(root.join("public/logo.png"), test_png()).expect("logo should be written");
        let engine = Engine::new(root.join("data"), root.join("cache"))
            .expect("test engine should be created");

        let analysis = analyze(&engine, "compose", root.to_string_lossy().as_ref())
            .expect("source should analyze");
        assert_eq!(analysis.detected_icon.as_deref(), Some("public/logo.png"));
        assert!(analysis
            .icon_preview_data_url
            .as_deref()
            .is_some_and(|preview| preview.starts_with("data:image/png;base64,")));
        fs::remove_dir_all(root).expect("test source should be removable");
    }

    #[test]
    fn custom_icon_data_is_normalized_for_portable_packages() {
        let data = format!("data:image/png;base64,{}", BASE64.encode(test_png()));
        let normalized = custom_icon_data(&data).expect("custom icon should normalize");
        let decoded = image::load_from_memory(&normalized).expect("icon should decode");
        assert_eq!((decoded.width(), decoded.height()), (1024, 1024));
    }

    #[test]
    fn generated_package_installs_a_custom_portable_icon() {
        let root = std::env::temp_dir().join(format!("packager-build-icon-{}", Uuid::new_v4()));
        let source = root.join("source");
        fs::create_dir_all(&source).expect("source should be created");
        fs::write(
            source.join("compose.yml"),
            "services:\n  app:\n    image: example/app\n    ports:\n      - '3000:3000'\n",
        )
        .expect("compose should be written");
        let engine = Engine::new(root.join("data"), root.join("cache"))
            .expect("test engine should be created");
        let icon_data = format!("data:image/png;base64,{}", BASE64.encode(test_png()));

        build(
            &engine,
            BuilderRequest {
                source_kind: "compose".into(),
                source: source.to_string_lossy().to_string(),
                id: "custom-icon-app".into(),
                name: "Custom Icon App".into(),
                description: String::new(),
                homepage: "https://example.com".into(),
                container_port: 3000,
                secret_keys: Vec::new(),
                icon_data: Some(icon_data),
            },
        )
        .expect("package should build");

        for icon in [
            root.join("data/created-packages/custom-icon-app/icon.png"),
            root.join("data/apps/custom-icon-app/definition/icon.png"),
        ] {
            let decoded = image::open(icon).expect("installed portable icon should decode");
            assert_eq!((decoded.width(), decoded.height()), (1024, 1024));
        }
        fs::remove_dir_all(root).expect("test package should be removable");
    }
}
