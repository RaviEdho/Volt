use crate::local::{CREATE_NO_WINDOW, read_tail, resolve_folder};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    os::windows::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent},
};

const ADDRESS: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3307));
const PASSWORD_MARKER: &str = "A temporary password is generated for root@localhost: ";

#[derive(Default)]
pub struct MySql {
    run: Option<Run>,
    ready: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    running: bool,
    ready: bool,
    error: Option<String>,
    logs: String,
    data_dir: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    status: Status,
    temporary_password: Option<String>,
}

struct Run {
    child: Child,
    workspace: TempDir,
    data_dir: PathBuf,
}

impl MySql {
    pub fn start(&mut self, storage: &Path, folder: &str) -> Started {
        let status = self.status(storage);
        if status.running {
            return Started {
                status,
                temporary_password: None,
            };
        }
        self.run = None;
        self.ready = false;
        let mut temporary_password = None;
        self.error = self
            .start_inner(storage, folder, &mut temporary_password)
            .err();
        Started {
            status: self.status(storage),
            temporary_password,
        }
    }

    fn start_inner(
        &mut self,
        storage: &Path,
        folder: &str,
        password: &mut Option<String>,
    ) -> Result<(), String> {
        let base = resolve_folder(folder, "MySQL")?;
        let executable = base.join("bin/mysqld.exe");
        if !executable.is_file() {
            return Err("Select the MySQL folder containing bin/mysqld.exe.".into());
        }
        verify_version(&executable)?;
        if accepts_connections() {
            return Err(format!(
                "{ADDRESS} is already in use. Stop the other server before starting Volt’s MySQL."
            ));
        }
        let parent = storage
            .parent()
            .ok_or("Could not locate the parent of Volt’s MySQL data folder.")?;
        create_managed_directory(parent)
            .map_err(|error| format!("Could not prepare Volt’s MySQL data folder: {error}"))?;
        *password = initialize_if_needed(parent, storage, &executable, &base)?;
        let mut run = Run::launch(&executable, &base, storage)?;
        let result = run.wait_for_ready().map_err(|error| match run.stop() {
            Ok(()) => error,
            Err(stop_error) => format!("{error} MySQL is still running: {stop_error}"),
        });
        self.ready = result.is_ok();
        self.run = Some(run);
        result
    }

    pub fn is_running(&mut self) -> bool {
        self.run
            .as_mut()
            .is_some_and(|run| !matches!(run.child.try_wait(), Ok(Some(_))))
    }

    pub fn stop(&mut self, storage: &Path) -> Status {
        if let Some(run) = &mut self.run {
            match run.stop() {
                Ok(()) => {
                    self.error = None;
                    self.ready = false;
                }
                Err(error) => self.error = Some(format!("Could not shut down MySQL: {error}")),
            }
        }
        self.status(storage)
    }

    pub fn status(&mut self, storage: &Path) -> Status {
        let mut running = false;
        let mut logs = String::new();
        if let Some(run) = &mut self.run {
            logs = run.logs();
            match run.child.try_wait() {
                Ok(None) => running = true,
                Ok(Some(exit)) => {
                    if self.ready {
                        self.error = Some(format!("MySQL exited unexpectedly ({exit})."));
                        self.ready = false;
                    }
                }
                Err(error) => {
                    running = true;
                    self.error = Some(format!("Could not check MySQL: {error}"));
                    self.ready = false;
                }
            }
        }
        Status {
            running,
            ready: self.ready,
            error: self.error.clone(),
            logs,
            data_dir: self
                .run
                .as_ref()
                .filter(|_| running)
                .map_or_else(|| storage.to_path_buf(), |run| run.data_dir.clone()),
        }
    }
}

fn create_managed_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    verify_directory(&fs::symlink_metadata(path)?)
}

fn verify_directory(metadata: &fs::Metadata) -> io::Result<()> {
    const REPARSE_POINT: u32 = 0x400;
    if !metadata.is_dir() || metadata.file_attributes() & REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected a regular directory, not a link or junction.",
        ));
    }
    Ok(())
}

fn server(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .arg("--no-defaults")
        .env_remove("MYSQLD_PARENT_PID")
        .env_remove("MYSQLD_WINDOWS_SERVICE")
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    command
}

fn option(name: &str, path: &Path) -> OsString {
    let mut value = OsString::from(name);
    value.push(path.as_os_str());
    value
}

fn verify_version(executable: &Path) -> Result<(), String> {
    let version = server(executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("Could not run MySQL: {error}"))?;
    let text = String::from_utf8_lossy(&version.stdout);
    // Use the version field, not a substring of the executable's filename.
    if !version.status.success()
        || !text
            .split_once(" Ver ")
            .is_some_and(|(_, version)| version.starts_with("8.4.") && !version.contains("MariaDB"))
    {
        return Err(
            "Volt supports MySQL 8.4 in this preview. Select a MySQL 8.4 installation.".into(),
        );
    }
    Ok(())
}

fn initialize_if_needed(
    storage: &Path,
    data: &Path,
    executable: &Path,
    base: &Path,
) -> Result<Option<String>, String> {
    match fs::symlink_metadata(data) {
        Ok(metadata) => {
            verify_directory(&metadata)
                .map_err(|error| format!("Invalid MySQL data folder: {error}"))?;
            if !data.join("mysql.ibd").is_file() {
                return Err(format!(
                    "{} exists but isn’t an initialized MySQL 8.4 data folder. Volt won’t replace it.",
                    data.display()
                ));
            }
            return Ok(None);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(format!("Could not inspect the MySQL data folder: {error}")),
    }

    // Initialize off to the side; a failed initialization never becomes the live data directory.
    let staging = tempfile::Builder::new()
        .prefix("initialize-")
        .tempdir_in(storage)
        .map_err(|error| format!("Could not create MySQL’s staging folder: {error}"))?;
    let result = server(executable)
        .current_dir(base.join("bin"))
        .arg(option("--basedir=", base))
        .arg(option("--datadir=", staging.path()))
        .args(["--initialize", "--console", "--lc-messages=en_US"])
        .output()
        .map_err(|error| format!("Could not initialize MySQL: {error}"))?;
    let output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&result.stderr),
        String::from_utf8_lossy(&result.stdout)
    );
    if !result.status.success() {
        let details = output
            .lines()
            .filter(|line| !line.contains(PASSWORD_MARKER))
            .collect::<Vec<_>>();
        let start = details.len().saturating_sub(6);
        return Err(format!(
            "MySQL initialization failed ({}).\n{}",
            result.status,
            details[start..].join("\n")
        ));
    }
    let password = output.lines()
        .find_map(|line| line.split_once(PASSWORD_MARKER).map(|(_, password)| password.trim()))
        .filter(|password| !password.is_empty())
        .ok_or("MySQL did not report a temporary root password. Volt did not keep the new data folder.")?
        .to_owned();
    fs::rename(staging.path(), data)
        .map_err(|error| format!("Could not save the initialized MySQL data folder: {error}"))?;
    Ok(Some(password))
}

impl Run {
    fn launch(executable: &Path, base: &Path, data: &Path) -> Result<Self, String> {
        let workspace = tempfile::Builder::new()
            .prefix("volt-mysql-")
            .tempdir()
            .map_err(|error| format!("Could not prepare MySQL’s log folder: {error}"))?;
        let log = fs::File::create(workspace.path().join("mysql.log"))
            .map_err(|error| format!("Could not open MySQL’s startup log: {error}"))?;
        let stderr = log.try_clone().map_err(|error| error.to_string())?;
        let child = server(executable)
            .current_dir(base.join("bin"))
            .arg(option("--basedir=", base))
            .arg(option("--datadir=", data))
            .args([
                "--no-monitor",
                "--console",
                "--bind-address=127.0.0.1",
                "--port=3307",
                "--mysqlx=OFF",
                "--skip-named-pipe",
                "--skip-shared-memory",
                "--skip-log-bin",
                "--persisted-globals-load=OFF",
                "--upgrade=NONE",
                "--lc-messages=en_US",
            ])
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| format!("Could not launch MySQL: {error}"))?;
        Ok(Self {
            child,
            workspace,
            data_dir: data.to_path_buf(),
        })
    }

    fn logs(&self) -> String {
        read_tail(&self.workspace.path().join("mysql.log")).unwrap_or_default()
    }

    fn wait_for_ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(25);
        while Instant::now() < deadline {
            if let Some(exit) = self.child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "MySQL exited during startup ({exit}). See the MySQL log."
                ));
            }
            if self.logs().contains("ready for connections") && accepts_connections() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(150));
        }
        Err(format!(
            "MySQL did not become ready at {ADDRESS} within 25 seconds."
        ))
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        // MySQL 8.4's Windows --no-monitor process listens on this per-PID event.
        // Signal only the child whose process handle Volt owns; don't kill a service or require root credentials.
        let name = format!("MYSQLShutdown{}\0", self.child.id())
            .encode_utf16()
            .collect::<Vec<_>>();
        let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
        if event.is_null() {
            let error = io::Error::last_os_error();
            return if self.child.try_wait()?.is_some() {
                Ok(())
            } else {
                Err(error)
            };
        }
        let sent = unsafe { SetEvent(event) };
        let error = if sent == 0 {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        unsafe { CloseHandle(event) };
        if let Some(error) = error {
            return Err(error);
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "MySQL is still shutting down; Volt did not force-kill it.",
        ))
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn accepts_connections() -> bool {
    TcpStream::connect_timeout(&ADDRESS, Duration::from_millis(150)).is_ok()
}

#[cfg(test)]
#[path = "mysql_smoke.rs"]
mod smoke;
