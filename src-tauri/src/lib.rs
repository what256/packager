use packager_core::{
    ActionResult, AppSummary, BuilderAnalysis, BuilderRequest, CatalogEntry, Engine,
    ManagedRuntimeStatus, SystemStatus,
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

fn engine(app: &AppHandle) -> Result<Engine, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Cannot locate Packager data: {error}"))?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Cannot locate Packager cache: {error}"))?;
    Engine::desktop(data, cache)
        .map(|engine| engine.with_launcher_icon(include_bytes!("../icons/icon.icns").to_vec()))
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

fn launch_deep_link(app: AppHandle, id: String) {
    tauri::async_runtime::spawn(async move {
        let start_app = app.clone();
        let start_id = id.clone();
        let started = tauri::async_runtime::spawn_blocking(move || {
            packager_core::start(&engine(&start_app)?, &start_id)
        })
        .await;
        if !matches!(started, Ok(Ok(_))) {
            return;
        }
        for _ in 0..120 {
            if open_app_window(app.clone(), id.clone()).is_ok() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });
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
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    if let Some(id) = deep_link_id(&url) {
                        launch_deep_link(handle.clone(), id);
                    }
                }
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    if let Some(id) = deep_link_id(&url) {
                        launch_deep_link(app.handle().clone(), id);
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_catalog,
            get_apps,
            get_system_status,
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
