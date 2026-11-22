use crate::{
    local::{CREATE_NO_WINDOW, read_tail, resolve_folder},
    projects::{self, Project, Site},
};
use serde::Serialize;
use std::{
    env, fs,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const PORT: u16 = 8080;
const ADDRESS: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT));
pub const DEMO_URL: &str = "http://127.0.0.1:8080/";
const STARTUP_LOG: &str = "startup.log";
const ERROR_LOG: &str = "apache.log";

#[derive(Default)]
pub struct Apache {
    run: Option<Run>,
    ready: bool,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct Status {
    running: bool,
    ready: bool,
    error: Option<String>,
    logs: String,
}

struct Run {
    child: Child,
    dir: TempDir,
    marker: String,
    project_hosts: Vec<String>,
}

impl Apache {
    pub fn start(&mut self, apache: &str, php: &str, projects: &[Project]) -> Status {
        let status = self.status();
        if status.running {
            return status;
        }
        self.run = None;
        self.ready = false;
        self.error = match Run::start(apache, php, projects) {
            Ok(mut run) => {
                let error = match run.wait_for_php() {
                    Ok(()) => {
                        self.ready = true;
                        None
                    }
                    Err(error) => Some(match run.stop() {
                        Ok(()) => error,
                        Err(stop_error) => format!("{error} Could not stop Apache: {stop_error}"),
                    }),
                };
                self.run = Some(run);
                error
            }
            Err(error) => Some(error),
        };
        self.status()
    }

    pub fn stop(&mut self) -> Status {
        if let Some(run) = &mut self.run {
            self.error = run
                .stop()
                .err()
                .map(|error| format!("Could not stop Apache: {error}"));
        }
        self.ready = false;
        self.status()
    }

    pub fn is_running(&mut self) -> bool {
        self.run
            .as_mut()
            .is_some_and(|run| !matches!(run.child.try_wait(), Ok(Some(_))))
    }

    pub fn status(&mut self) -> Status {
        let mut running = false;
        let mut logs = String::new();
        if let Some(run) = &mut self.run {
            match run.child.try_wait() {
                Ok(None) => running = true,
                Ok(Some(exit)) => {
                    if self.ready {
                        self.error = Some(format!("Apache exited unexpectedly ({exit})."));
                    }
                    self.ready = false;
                }
                Err(error) => {
                    running = true; // Keep Stop available when process state is uncertain.
                    self.ready = false;
                    self.error = Some(format!("Could not check Apache: {error}"));
                }
            }
            logs = [STARTUP_LOG, ERROR_LOG]
                .into_iter()
                .filter_map(|name| read_tail(&run.dir.path().join(name)).ok())
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
        }
        Status {
            running,
            ready: self.ready,
            error: self.error.clone(),
            logs,
        }
    }

    pub fn project_address(&mut self, slug: &str) -> Result<(String, u16), String> {
        if !self.status().ready {
            return Err("Start Apache and PHP before opening a project.".into());
        }
        let host = format!("{slug}.test");
        if !self
            .run
            .as_ref()
            .is_some_and(|run| run.project_hosts.contains(&host))
        {
            return Err("That project isn’t configured in the running stack. Restart Apache after adding it.".into());
        }
        Ok((host, PORT))
    }

    pub fn open_demo(&mut self) -> Result<(), String> {
        if !self.status().ready {
            return Err("Start Apache and PHP before opening the demo.".into());
        }
        open::that_detached(DEMO_URL).map_err(|error| format!("Could not open the demo: {error}"))
    }
}

impl Run {
    fn start(apache: &str, php: &str, projects: &[Project]) -> Result<Self, String> {
        let apache = resolve_folder(apache, "Apache")?;
        let php = resolve_folder(php, "PHP")?;
        let sites = projects::resolve(projects)?;
        let httpd = apache.join("bin/httpd.exe");
        for file in [
            &httpd,
            &apache.join("modules/mod_authz_core.so"),
            &apache.join("modules/mod_dir.so"),
            &apache.join("modules/mod_mime.so"),
            &apache.join("conf/mime.types"),
        ] {
            if !file.is_file() {
                return Err(format!(
                    "Missing {}. Select the Apache root folder (the one that contains bin and modules).",
                    file.display()
                ));
            }
        }
        for file in [php.join("php8ts.dll"), php.join("php8apache2_4.dll")] {
            if !file.is_file() {
                return Err(format!(
                    "Missing {}. Use a Thread Safe PHP 8 build that matches Apache’s architecture.",
                    file.display()
                ));
            }
        }
        if TcpStream::connect_timeout(&ADDRESS, Duration::from_millis(200)).is_ok() {
            return Err(format!(
                "{ADDRESS} is already in use. Stop the other server before starting Volt."
            ));
        }

        let dir = tempfile::Builder::new()
            .prefix("volt-apache-")
            .tempdir()
            .map_err(|error| format!("Could not create Apache's temporary workspace: {error}"))?;
        let marker = format!(
            "volt-php-{}",
            dir.path().file_name().unwrap().to_string_lossy()
        );
        let config = prepare_workspace(&dir, &apache, &php, &marker, &sites)
            .map_err(|error| format!("Could not prepare Apache's configuration: {error}"))?;
        let path = env::var_os("PATH").unwrap_or_default();
        let path = env::join_paths(std::iter::once(php).chain(env::split_paths(&path)))
            .map_err(|error| format!("Could not add PHP to Apache's PATH: {error}"))?;
        let startup_log = fs::File::create(dir.path().join(STARTUP_LOG))
            .map_err(|error| format!("Could not open Apache's startup log: {error}"))?;
        let stderr = startup_log.try_clone().map_err(|error| error.to_string())?;

        // In -X mode the Windows MPM runs in a single process, not as a service.
        let child = Command::new(&httpd)
            .args(["-X", "-f"])
            .arg(config)
            .current_dir(apache.join("bin"))
            .env("PATH", path)
            .env("PHP_INI_SCAN_DIR", dir.path().join("ini-scan"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(startup_log))
            .stderr(Stdio::from(stderr))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("Could not launch {}: {error}", httpd.display()))?;
        let project_hosts = sites.into_iter().map(|site| site.host).collect();
        Ok(Self {
            child,
            dir,
            marker,
            project_hosts,
        })
    }

    fn wait_for_php(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Some(exit) = self.child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "Apache exited during startup ({exit}). Check the logs and that both builds have the required Visual C++ runtime."
                ));
            }
            if php_responds(&self.marker).unwrap_or(false) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!(
            "PHP did not answer at {DEMO_URL} within 8 seconds. Check the Apache logs."
        ))
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_none() {
            if let Err(error) = self.child.kill() {
                // The process may have exited between try_wait and kill.
                if self.child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
            self.child.wait()?;
        }
        Ok(())
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn config_path(path: &Path) -> io::Result<String> {
    let path = path.to_str().unwrap_or_default().replace('\\', "/");
    if path.is_empty() || path.contains(['"', '\n', '\r']) || path.contains("${") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A folder path contains characters Apache cannot safely use in configuration.",
        ));
    }
    Ok(path)
}

fn prepare_workspace(
    dir: &TempDir,
    apache: &Path,
    php: &Path,
    marker: &str,
    sites: &[Site],
) -> io::Result<PathBuf> {
    let root = dunce::canonicalize(dir.path())?;
    let config = root.join("httpd.conf");
    fs::create_dir(root.join("www"))?;
    fs::create_dir(root.join("ini-scan"))?;
    // Reverse the marker in source so the readiness check only passes if PHP runs it.
    let reversed: String = marker.chars().rev().collect();
    fs::write(
        root.join("www/index.php"),
        format!(
            "<!doctype html><meta charset=\"utf-8\"><title>Volt PHP demo</title><h1>Volt PHP is running</h1><p>PHP <?= PHP_VERSION ?></p><!-- <?= strrev('{reversed}') ?> -->"
        ),
    )?;
    let mut ini = "expose_php=Off\ndisplay_errors=Off\nlog_errors=On\n".to_owned();
    if php.join("ext/php_pdo_mysql.dll").is_file() {
        let extension_dir = config_path(&php.join("ext"))?;
        ini.push_str(&format!(
            "extension_dir=\"{extension_dir}\"\nextension=php_pdo_mysql.dll\n"
        ));
    }
    fs::write(root.join("php.ini"), ini)?;
    let apache = config_path(apache)?;
    let php = config_path(php)?;
    let root = config_path(&root)?;
    let mut contents = format!(
        r#"ServerRoot "{apache}"
Listen {ADDRESS}
ServerName {ADDRESS}
PidFile "{root}/httpd.pid"
ErrorLog "{root}/{ERROR_LOG}"
LogLevel warn
LoadModule authz_core_module modules/mod_authz_core.so
LoadModule dir_module modules/mod_dir.so
LoadModule mime_module modules/mod_mime.so
TypesConfig conf/mime.types
LoadFile "{php}/php8ts.dll"
LoadModule php_module "{php}/php8apache2_4.dll"
PHPIniDir "{root}"
DocumentRoot "{root}/www"
<Directory "/">
    Options None
    AllowOverride None
    Require all denied
</Directory>
<LocationMatch "(^|/)\.">
    Require all denied
</LocationMatch>
<VirtualHost {ADDRESS}>
    ServerName volt.invalid
    <Location "/">
        Require all denied
    </Location>
</VirtualHost>
"#
    );
    contents.push_str(&virtual_host("127.0.0.1", &format!("{root}/www"), false));
    for site in sites {
        contents.push_str(&virtual_host(
            &site.host,
            &config_path(&site.document_root)?,
            site.document_root.join("index.php").is_file(),
        ));
    }
    fs::write(&config, contents)?;
    Ok(config)
}

fn virtual_host(host: &str, web_root: &str, front_controller: bool) -> String {
    let fallback = if front_controller {
        "        FallbackResource /index.php\n"
    } else {
        ""
    };
    let aliases = if host == "127.0.0.1" {
        "    ServerAlias localhost\n"
    } else {
        ""
    };
    format!(
        r#"<VirtualHost {ADDRESS}>
    ServerName {host}
{aliases}    DocumentRoot "{web_root}"
    <Directory "{web_root}">
        Options None
        AllowOverride None
        Require all granted
        DirectoryIndex index.php index.html
{fallback}        <FilesMatch "(?i)\.php$">
            SetHandler application/x-httpd-php
        </FilesMatch>
        <FilesMatch "(?i)\.(?:phar|phps|phtml|php[0-9]+)(?:\.|$)|\.php\.">
            Require all denied
        </FilesMatch>
    </Directory>
</VirtualHost>
"#
    )
}

fn php_responds(marker: &str) -> io::Result<bool> {
    let timeout = Duration::from_millis(250);
    let mut stream = TcpStream::connect_timeout(&ADDRESS, timeout)?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET / HTTP/1.0\r\nHost: {ADDRESS}\r\nConnection: close\r\n\r\n"
    )?;
    let deadline = Instant::now() + timeout;
    let mut response = Vec::new();
    let mut buffer = [0; 1024];
    while response.len() < 8192 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        stream.set_read_timeout(Some(remaining))?;
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..count]);
    }
    let response = String::from_utf8_lossy(&response);
    Ok(response
        .split_once("\r\n\r\n")
        .is_some_and(|(headers, body)| {
            headers
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                == Some("200")
                && body.contains(&format!("<!-- {marker} -->"))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apache_paths_preserve_spaces_and_apostrophes() {
        let path = Path::new(r"D:\EDHO'S FILES\Volt Demo");
        assert_eq!(config_path(path).unwrap(), "D:/EDHO'S FILES/Volt Demo");
        assert!(config_path(Path::new(r"C:\${SERVER_ROOT}\php")).is_err());
        assert!(config_path(Path::new("C:/a\"b")).is_err());
    }

    #[test]
    fn generated_workspace_has_a_php_only_marker_and_loopback_listener() {
        let dir = tempfile::tempdir().unwrap();
        let marker = "volt-php-test";
        let config = prepare_workspace(
            &dir,
            Path::new(r"C:\Apache 2.4"),
            Path::new(r"C:\Edho's PHP"),
            marker,
            &[Site {
                host: "blog.test".into(),
                document_root: PathBuf::from(r"D:\Edho's Projects\Blog App\public"),
            }],
        )
        .unwrap();
        let config = fs::read_to_string(config).unwrap();
        assert!(config.contains("Listen 127.0.0.1:8080\n"));
        assert!(config.contains("ServerName blog.test\n"));
        assert!(config.contains("DocumentRoot \"D:/Edho's Projects/Blog App/public\""));
        assert!(config.find("ServerName volt.invalid") < config.find("ServerName blog.test"));
        assert!(config.contains("LoadModule php_module \"C:/Edho's PHP/php8apache2_4.dll\""));
        let demo = fs::read_to_string(dir.path().join("www/index.php")).unwrap();
        assert!(demo.contains("<?= PHP_VERSION ?>"));
        assert!(!demo.contains(marker));
    }
}
