use packager_core::{
    ActionResult, AppSummary, BuilderAnalysis, BuilderRequest, CatalogEntry, Engine,
    ManagedRuntimeStatus, SystemStatus,
};
use std::fs;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

fn engine(_app: &AppHandle) -> Result<Engine, String> {
    Engine::from_environment().map(|engine| {
        engine
            .with_launcher_installation()
            .with_launcher_icon(include_bytes!("../icons/icon.icns").to_vec())
    })
}

fn launcher_app_id() -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    let marker = executable
        .parent()?
        .parent()?
        .join("Resources/packager-launcher-id");
    let id = fs::read_to_string(marker).ok()?.trim().to_owned();
    if !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        Some(id)
    } else {
        None
    }
}

fn deep_link_id(url: &Url) -> Option<String> {
    let id = url.path().trim_matches('/');
    if url.scheme() == "packager"
        && url.host_str() == Some("open")
        && !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        Some(id.into())
    } else {
        None
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

async fn blocking<F, T>(operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("Background operation failed: {error}"))?
}

#[tauri::command]
async fn get_catalog(app: AppHandle) -> Result<Vec<CatalogEntry>, String> {
    blocking(move || packager_core::catalog(&engine(&app)?)).await
}

#[tauri::command]
async fn get_apps(app: AppHandle) -> Result<Vec<AppSummary>, String> {
    blocking(move || packager_core::list_apps(&engine(&app)?)).await
}

#[tauri::command]
async fn get_system_status(app: AppHandle) -> Result<SystemStatus, String> {
    blocking(move || packager_core::system_status(&engine(&app)?)).await
}

#[tauri::command]
fn get_launcher_app_id() -> Option<String> {
    launcher_app_id()
}

#[tauri::command]
async fn install_managed_runtime(app: AppHandle) -> Result<ManagedRuntimeStatus, String> {
    blocking(move || packager_core::install_runtime(&engine(&app)?)).await
}

#[tauri::command]
async fn start_managed_runtime(app: AppHandle) -> Result<ManagedRuntimeStatus, String> {
    blocking(move || packager_core::start_runtime(&engine(&app)?)).await
}

#[tauri::command]
async fn stop_managed_runtime(app: AppHandle) -> Result<ManagedRuntimeStatus, String> {
    blocking(move || packager_core::stop_runtime(&engine(&app)?)).await
}

#[tauri::command]
async fn install_app(app: AppHandle, id: String) -> Result<ActionResult, String> {
    blocking(move || packager_core::install(&engine(&app)?, &id)).await
}

#[tauri::command]
async fn start_app(app: AppHandle, id: String) -> Result<ActionResult, String> {
    blocking(move || packager_core::start(&engine(&app)?, &id)).await
}

#[tauri::command]
async fn stop_app(app: AppHandle, id: String) -> Result<ActionResult, String> {
    blocking(move || packager_core::stop(&engine(&app)?, &id)).await
}

#[tauri::command]
async fn update_app(app: AppHandle, id: String) -> Result<ActionResult, String> {
    blocking(move || packager_core::update(&engine(&app)?, &id)).await
}

#[tauri::command]
async fn set_automatic_updates(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<ActionResult, String> {
    blocking(move || packager_core::set_automatic_updates(&engine(&app)?, &id, enabled)).await
}

#[tauri::command]
async fn get_app_logs(app: AppHandle, id: String, lines: u32) -> Result<String, String> {
    blocking(move || packager_core::logs(&engine(&app)?, &id, lines)).await
}

#[tauri::command]
fn open_app_window(app: AppHandle, id: String) -> Result<ActionResult, String> {
    let engine = engine(&app)?;
    let url = Url::parse(&packager_core::app_url(&engine, &id)?)
        .map_err(|error| format!("Invalid packaged app URL: {error}"))?;
    let label = format!("app-{id}");
    if let Some(window) = app.get_webview_window(&label) {
        window
            .navigate(url.clone())
            .map_err(|error| format!("Cannot reload packaged app: {error}"))?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
    } else {
        let title = packager_core::list_apps(&engine)?
            .into_iter()
            .find(|item| item.id == id)
            .map(|item| item.name)
            .unwrap_or_else(|| id.clone());
        WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url))
            .title(title)
            .inner_size(1280.0, 820.0)
            .min_inner_size(720.0, 520.0)
            .center()
            .build()
            .map_err(|error| format!("Cannot open app window: {error}"))?;
    }
    Ok(ActionResult {
        id,
        status: "ready".into(),
        message: "Packaged app opened".into(),
    })
}

#[tauri::command]
async fn uninstall_app(
    app: AppHandle,
    id: String,
    delete_data: bool,
) -> Result<ActionResult, String> {
    blocking(move || packager_core::uninstall(&engine(&app)?, &id, delete_data)).await
}

#[tauri::command]
async fn run_automatic_updates(app: AppHandle) -> Result<Vec<ActionResult>, String> {
    blocking(move || packager_core::automatic_updates(&engine(&app)?)).await
}

#[tauri::command]
async fn import_package(app: AppHandle, source_dir: String) -> Result<ActionResult, String> {
    blocking(move || packager_core::import_package(&engine(&app)?, &source_dir)).await
}

#[tauri::command]
async fn analyze_package_source(
    app: AppHandle,
    source_kind: String,
    source: String,
) -> Result<BuilderAnalysis, String> {
    blocking(move || packager_core::analyze(&engine(&app)?, &source_kind, &source)).await
}

#[tauri::command]
async fn build_package(app: AppHandle, request: BuilderRequest) -> Result<ActionResult, String> {
    blocking(move || packager_core::build(&engine(&app)?, request)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            let is_launcher = launcher_app_id().is_some();
            if is_launcher {
                hide_main_window(app.handle());
            } else if let Ok(packager) = engine(app.handle()) {
                let _ = packager_core::refresh_installed_packages(&packager);
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                if event.urls().iter().any(|url| deep_link_id(url).is_some()) {
                    hide_main_window(&handle);
                }
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                if urls.iter().any(|url| deep_link_id(url).is_some()) {
                    hide_main_window(app.handle());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_catalog,
            get_apps,
            get_system_status,
            get_launcher_app_id,
            install_managed_runtime,
            start_managed_runtime,
            stop_managed_runtime,
            install_app,
            start_app,
            stop_app,
            update_app,
            set_automatic_updates,
            get_app_logs,
            open_app_window,
            uninstall_app,
            run_automatic_updates,
            import_package,
            analyze_package_source,
            build_package,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Packager");
}
