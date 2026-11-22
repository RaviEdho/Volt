use std::{
    env, io,
    path::{Path, PathBuf},
};

pub struct Layout(PathBuf);

impl Layout {
    pub fn resolve() -> io::Result<Self> {
        if tauri::is_dev() {
            // Keep development data separate from both the packaged app and build artifacts.
            let source = Path::new(env!("CARGO_MANIFEST_DIR"));
            return Ok(Self(source.with_file_name(".volt-dev")));
        }
        let exe = dunce::canonicalize(env::current_exe()?)?;
        Self::beside_executable(&exe)
    }

    fn beside_executable(exe: &Path) -> io::Result<Self> {
        exe.parent()
            .map(|parent| Self(parent.to_path_buf()))
            .ok_or_else(|| io::Error::other("Could not locate Volt’s installation folder."))
    }

    pub fn runtimes(&self) -> PathBuf {
        self.0.join("runtimes")
    }

    pub fn mysql_data(&self, version: Option<&str>) -> PathBuf {
        let data = self.0.join("data");
        match version.filter(|v| crate::runtime::Runtime::Mysql.supports(v)) {
            Some(version) => data.join("mysql-versions").join(version),
            None => data.join("mysql"),
        }
    }

    pub fn webview_settings(&self) -> PathBuf {
        self.0.join("config").join("webview")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_follows_the_executable_instead_of_the_working_directory() {
        let location = Path::new(r"D:\Custom Folder\Volt");
        let layout = Layout::beside_executable(&location.join("volt.exe")).unwrap();
        assert_eq!(layout.runtimes(), location.join("runtimes"));
        assert_eq!(layout.mysql_data(None), location.join(r"data\mysql"));
        assert_eq!(
            layout.mysql_data(Some("8.4.13")),
            location.join(r"data\mysql-versions\8.4.13")
        );
        assert_ne!(
            layout.mysql_data(Some("8.4.13")),
            layout.mysql_data(Some("8.4.6"))
        );
        assert_eq!(layout.webview_settings(), location.join(r"config\webview"));
    }
}
