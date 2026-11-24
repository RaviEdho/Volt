#[cfg(windows)]
mod apache;
#[cfg(windows)]
mod hosts;
#[cfg(windows)]
mod layout;
#[cfg(windows)]
mod local;
#[cfg(windows)]
mod mysql;
#[cfg(windows)]
mod projects;
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
mod user_path;

#[cfg(windows)]
mod desktop {
    use crate::{
        apache::{Apache, Status},
        hosts,
        layout::Layout,
        mysql,
        projects::{self, Project},
        runtime::{self, LEGACY, Runtime, Selection},
        user_path,
    };
    use std::{
        net::ToSocketAddrs,
        path::{Path, PathBuf},
        sync::Mutex,
    };
    use tauri::{AppHandle, Manager, RunEvent, WebviewWindowBuilder};

    async fn apache_task<T, F>(app: AppHandle, task: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut Apache) -> Result<T, String> + Send + 'static,
    {
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<Mutex<Apache>>();
            let mut apache = state
                .lock()
                .map_err(|_| "Apache controller is unavailable.")?;
            task(&mut apache)
        })
        .await
        .map_err(|error| error.to_string())?
    }

    #[tauri::command]
    async fn start_apache(app: AppHandle, projects: Vec<Project>) -> Result<Status, String> {
        environment_task(app, move |app, root| {
            let selection = runtime::load_selection(root)?;
            let (apache_dir, _) = resolve_runtime(root, Runtime::Apache, &selection)?;
            let (php_dir, _) = resolve_runtime(root, Runtime::Php, &selection)?;
            let state = app.state::<Mutex<Apache>>();
            let mut apache = state
                .lock()
                .map_err(|_| "Apache controller is unavailable.")?;
            Ok(apache.start(&apache_dir, &php_dir, &projects))
        })
        .await
    }

    #[tauri::command]
    async fn stop_apache(app: AppHandle) -> Result<Status, String> {
        apache_task(app, |apache| Ok(apache.stop())).await
    }

    #[tauri::command]
    async fn apache_status(app: AppHandle) -> Result<Status, String> {
        apache_task(app, |apache| Ok(apache.status())).await
    }

    #[tauri::command]
    async fn open_demo(app: AppHandle) -> Result<(), String> {
        apache_task(app, |apache| apache.open_demo()).await
    }

    #[tauri::command]
    async fn validate_projects(projects: Vec<Project>) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || projects::resolve(&projects).map(|_| ()))
            .await
            .map_err(|error| error.to_string())?
    }

    #[tauri::command]
    async fn sync_project_hosts(slugs: Vec<String>) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || hosts::sync(&slugs))
            .await
            .map_err(|error| error.to_string())?
    }

    #[tauri::command]
    async fn open_project(app: AppHandle, slug: String) -> Result<(), String> {
        let address = apache_task(app, move |apache| apache.project_address(&slug)).await?;
        tauri::async_runtime::spawn_blocking(move || {
            let resolved = (address.0.as_str(), address.1).to_socket_addrs().ok();
            let resolves_locally = resolved.is_some_and(|mut addresses| {
                let is_ipv4_loopback =
                    |addr: std::net::SocketAddr| addr.ip() == std::net::Ipv4Addr::LOCALHOST;
                addresses.next().is_some_and(is_ipv4_loopback) && addresses.all(is_ipv4_loopback)
            });
            if !resolves_locally {
                return Err(format!(
                    "{} doesn’t point to Volt on 127.0.0.1. Sync .test Domains and try again.",
                    address.0
                ));
            }
            let url = format!("http://{}:{}/", address.0, address.1);
            open::that_detached(url).map_err(|error| format!("Could not open the project: {error}"))
        })
        .await
        .map_err(|error| error.to_string())?
    }

    async fn mysql_task<T, F>(app: AppHandle, task: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut mysql::MySql, &std::path::Path) -> T + Send + 'static,
    {
        let layout = app.state::<Layout>();
        let root = layout.runtimes();
        let storage = runtime::read_selection(&root).unwrap_or_default();
        let version = runtime::active_version(&root, Runtime::Mysql, &storage);
        let storage = layout.mysql_data(version.as_deref());
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<Mutex<mysql::MySql>>();
            let mut mysql = state
                .lock()
                .map_err(|_| "MySQL controller is unavailable.")?;
            Ok(task(&mut mysql, &storage))
        })
        .await
        .map_err(|error| error.to_string())?
    }

    #[tauri::command]
    async fn start_mysql(app: AppHandle) -> Result<mysql::Started, String> {
        environment_task(app, move |app, root| {
            let selection = runtime::load_selection(root)?;
            let (folder, version) = resolve_runtime(root, Runtime::Mysql, &selection)?;
            let data = app.state::<Layout>().mysql_data(Some(&version));
            let state = app.state::<Mutex<mysql::MySql>>();
            let mut mysql = state
                .lock()
                .map_err(|_| "MySQL controller is unavailable.")?;
            Ok(mysql.start(&data, &folder))
        })
        .await
    }

    #[tauri::command]
    async fn stop_mysql(app: AppHandle) -> Result<mysql::Status, String> {
        mysql_task(app, mysql::MySql::stop).await
    }

    #[tauri::command]
    async fn mysql_status(app: AppHandle) -> Result<mysql::Status, String> {
        mysql_task(app, mysql::MySql::status).await
    }

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EnvironmentStatus {
        root: PathBuf,
        runtimes: Vec<runtime::RuntimeStatus>,
        path_enabled: bool,
        path_error: Option<String>,
    }

    #[derive(Default)]
    struct EnvironmentLock(Mutex<()>);

    fn environment_info(root: &Path, choice: Option<bool>) -> Result<EnvironmentStatus, String> {
        let selection = runtime::load_selection(root)?;
        let (path_enabled, path_error) = match user_path::sync(root, choice, &selection) {
            Ok(enabled) => (enabled, None),
            Err(error) => (
                user_path::enabled(root).unwrap_or(false),
                Some(format!("Could not update the Windows user PATH: {error}")),
            ),
        };
        Ok(EnvironmentStatus {
            root: root.to_path_buf(),
            runtimes: runtime::status(root, &selection),
            path_enabled,
            path_error,
        })
    }

    fn resolve_runtime(
        root: &Path,
        id: Runtime,
        selection: &Selection,
    ) -> Result<(String, String), String> {
        let version = runtime::active_version(root, id, selection)
            .ok_or_else(|| format!("Install {} and select an active version first.", id.name()))?;
        let folder = id.installed_folder(root, &version).ok_or_else(|| {
            format!(
                "The active {} version is missing. Choose an installed version.",
                id.name()
            )
        })?;
        verify_folder_version(id, &version, &folder)?;
        Ok((folder.display().to_string(), version))
    }

    fn verify_folder_version(id: Runtime, version: &str, folder: &Path) -> Result<(), String> {
        if version != LEGACY {
            let detected = id.detect_version(folder)?;
            if detected != version {
                return Err(format!(
                    "The {} folder is named {version}, but the binary reports {detected}.",
                    id.name()
                ));
            }
        }
        Ok(())
    }

    async fn environment_task<T: Send + 'static>(
        app: AppHandle,
        task: impl FnOnce(&AppHandle, &Path) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let root = app.state::<Layout>().runtimes();
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<EnvironmentLock>();
            let _guard = state
                .0
                .lock()
                .map_err(|_| "Environment manager is unavailable.")?;
            task(&app, &root)
        })
        .await
        .map_err(|error| error.to_string())?
    }

    #[tauri::command]
    async fn environment_status(app: AppHandle) -> Result<EnvironmentStatus, String> {
        environment_task(app, |_, root| environment_info(root, None)).await
    }

    #[tauri::command]
    async fn install_runtime(
        app: AppHandle,
        component: Runtime,
        version: Option<String>,
        source: Option<String>,
    ) -> Result<EnvironmentStatus, String> {
        environment_task(app, move |_, root| {
            runtime::load_selection(root)?;
            runtime::install(root, component, version, source)?;
            environment_info(root, None)
        })
        .await
    }

    #[tauri::command]
    async fn install_php_composer(app: AppHandle) -> Result<EnvironmentStatus, String> {
        environment_task(app, |_, root| {
            let selection = runtime::load_selection(root)?;
            let php = runtime::active_folder(root, Runtime::Php, &selection)
                .ok_or("Select an installed PHP version first.")?;
            runtime::install_composer(&php)?;
            environment_info(root, None)
        })
        .await
    }

    #[tauri::command]
    async fn select_runtime(
        app: AppHandle,
        component: Runtime,
        version: String,
    ) -> Result<EnvironmentStatus, String> {
        environment_task(app, move |app, root| {
            let running = match component {
                Runtime::Mysql => app
                    .state::<Mutex<mysql::MySql>>()
                    .lock()
                    .map_err(|_| "MySQL controller is unavailable.")?
                    .is_running(),
                _ => app
                    .state::<Mutex<Apache>>()
                    .lock()
                    .map_err(|_| "Apache controller is unavailable.")?
                    .is_running(),
            };
            if running {
                return Err("Stop the affected service before switching versions.".into());
            }
            let folder = component
                .installed_folder(root, &version)
                .ok_or("Choose an installed version.")?;
            verify_folder_version(component, &version, &folder)?;
            let mut selection = runtime::load_selection(root)?;
            selection.insert(component, version);
            runtime::save_selection(root, &selection)?;
            environment_info(root, None)
        })
        .await
    }

    #[tauri::command]
    async fn set_environment_path(
        app: AppHandle,
        enabled: bool,
    ) -> Result<EnvironmentStatus, String> {
        environment_task(app, move |_, root| environment_info(root, Some(enabled))).await
    }

    #[tauri::command]
    async fn open_runtime_folder(app: AppHandle) -> Result<(), String> {
        environment_task(app, |_, root| {
            runtime::prepare(root)?;
            open::that_detached(root).map_err(|error| error.to_string())
        })
        .await
    }

    pub fn run() {
        let layout = Layout::resolve().expect("could not locate Volt’s installation folder");
        let mut context = tauri::generate_context!();
        // Create the configured window ourselves so its WebView2 profile follows the install directory.
        context.config_mut().app.windows[0].create = false;
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .manage(layout)
            .manage(EnvironmentLock::default())
            .setup(|app| {
                let directory = app.state::<Layout>().webview_settings();
                std::fs::create_dir_all(&directory)?;
                WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                    .data_directory(dunce::canonicalize(directory)?)
                    .build()?;
                Ok(())
            })
            .manage(Mutex::new(Apache::default()))
            .manage(Mutex::new(mysql::MySql::default()))
            .invoke_handler(tauri::generate_handler![
                start_apache,
                stop_apache,
                apache_status,
                open_demo,
                validate_projects,
                sync_project_hosts,
                open_project,
                start_mysql,
                stop_mysql,
                mysql_status,
                environment_status,
                install_runtime,
                install_php_composer,
                select_runtime,
                set_environment_path,
                open_runtime_folder
            ])
            .build(context)
            .expect("failed to build Volt")
            .run(|app, event| {
                if let RunEvent::Exit = event {
                    let state = app.state::<Mutex<Apache>>();
                    if let Ok(mut apache) = state.lock() {
                        // Don't leave the web server accepting new requests during MySQL shutdown.
                        *apache = Apache::default();
                    }
                    let state = app.state::<Mutex<mysql::MySql>>();
                    if let Ok(mut mysql) = state.lock() {
                        *mysql = mysql::MySql::default();
                    }
                }
            });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    desktop::run();
    #[cfg(not(windows))]
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to launch Volt");
}
