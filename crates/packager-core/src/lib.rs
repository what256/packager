//! Tauri-independent Packager engine used by both the desktop app and CLI.

mod builder;
mod managed_runtime;
mod model;
mod runtime;
mod secret_store;

use std::{env, fs, path::PathBuf};

pub use builder::{analyze, build};
pub use managed_runtime::{
    docker_command, ensure_running as ensure_runtime_running, install as install_runtime,
    start as start_runtime, status as runtime_status, stop as stop_runtime,
    uninstall as uninstall_runtime,
};
pub use model::*;
pub use runtime::{
    app_url, automatic_updates, catalog, import_package, install, list_apps, logs,
    refresh_installed_packages, set_automatic_updates, start, stop, system_status, uninstall,
    update,
};

/// Filesystem roots are explicit so every client uses the same engine without
/// depending on a GUI framework's path resolver.
#[derive(Debug, Clone)]
pub struct Engine {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    install_launchers: bool,
    launcher_icon: Option<Vec<u8>>,
}

impl Engine {
    pub fn new(
        data_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let engine = Self {
            data_dir: data_dir.into(),
            cache_dir: cache_dir.into(),
            install_launchers: false,
            launcher_icon: None,
        };
        fs::create_dir_all(&engine.data_dir)
            .map_err(|error| format!("Cannot create Packager data directory: {error}"))?;
        fs::create_dir_all(&engine.cache_dir)
            .map_err(|error| format!("Cannot create Packager cache directory: {error}"))?;
        Ok(engine)
    }

    pub fn desktop(
        data_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let mut engine = Self::new(data_dir, cache_dir)?;
        engine.install_launchers = true;
        Ok(engine)
    }

    pub fn from_environment() -> Result<Self, String> {
        if let Some(data) = env::var_os("PACKAGER_DATA_DIR") {
            let data = PathBuf::from(data);
            let cache = env::var_os("PACKAGER_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| data.join("cache"));
            return Self::new(data, cache);
        }

        #[cfg(target_os = "macos")]
        let (data, cache) = {
            let home = env::var_os("HOME").ok_or("Cannot locate your home directory")?;
            let home = PathBuf::from(home);
            (
                home.join("Library/Application Support/dev.packager.desktop"),
                home.join("Library/Caches/dev.packager.desktop"),
            )
        };
        #[cfg(target_os = "windows")]
        let (data, cache) = {
            let roaming = env::var_os("APPDATA").ok_or("Cannot locate APPDATA")?;
            let local = env::var_os("LOCALAPPDATA").ok_or("Cannot locate LOCALAPPDATA")?;
            (
                PathBuf::from(roaming).join("dev.packager.desktop"),
                PathBuf::from(local).join("dev.packager.desktop"),
            )
        };
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        let (data, cache) = {
            let home = env::var_os("HOME").ok_or("Cannot locate your home directory")?;
            let home = PathBuf::from(home);
            let data = env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"))
                .join("packager");
            let cache = env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cache"))
                .join("packager");
            (data, cache)
        };
        Self::new(data, cache)
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn cache_dir(&self) -> &std::path::Path {
        &self.cache_dir
    }

    pub(crate) fn launchers_enabled(&self) -> bool {
        self.install_launchers
    }

    pub fn with_launcher_icon(mut self, icon: impl Into<Vec<u8>>) -> Self {
        self.launcher_icon = Some(icon.into());
        self
    }

    pub(crate) fn launcher_icon(&self) -> Option<&[u8]> {
        self.launcher_icon.as_deref()
    }
}
