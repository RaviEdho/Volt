use crate::local::CREATE_NO_WINDOW;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read},
    os::windows::{fs::MetadataExt, process::CommandExt},
    path::{Component, Path, PathBuf},
    process::Command,
};

#[path = "runtime_catalog.rs"]
mod catalog;
use catalog::Package;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Php,
    Apache,
    Mysql,
}

pub const RUNTIMES: [Runtime; 3] = [Runtime::Php, Runtime::Apache, Runtime::Mysql];

pub const LEGACY: &str = "legacy";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    id: Runtime,
    folder: PathBuf,
    downloads: Vec<&'static str>,
    versions: Vec<String>,
    active_version: Option<String>,
    active_folder: Option<PathBuf>,
}

impl Runtime {
    pub fn name(self) -> &'static str {
        match self {
            Self::Php => "php",
            Self::Apache => "apache",
            Self::Mysql => "mysql",
        }
    }

    pub fn folder(self, root: &Path, version: &str) -> Option<PathBuf> {
        let base = root.join(self.name());
        if version == LEGACY {
            return Some(base);
        }
        self.supports(version).then(|| base.join(version))
    }

    pub fn binaries_dir(self, folder: &Path) -> PathBuf {
        match self {
            Self::Php => folder.to_path_buf(),
            _ => folder.join("bin"),
        }
    }

    pub fn supports(self, version: &str) -> bool {
        let Some((major, minor, _)) = version_parts(version) else {
            return false;
        };
        match self {
            Self::Php => major == 8,
            Self::Apache => (major, minor) == (2, 4),
            Self::Mysql => (major, minor) == (8, 4),
        }
    }

    pub fn installed_folder(self, root: &Path, version: &str) -> Option<PathBuf> {
        let folder = self.folder(root, version)?;
        (!is_link(root) && !is_link(&root.join(self.name())) && self.valid_layout(&folder))
            .then_some(folder)
    }

    fn required_files(self) -> &'static [&'static str] {
        match self {
            Self::Php => &[
                "php.exe",
                "php8ts.dll",
                "php8apache2_4.dll",
                "ext/php_pdo_mysql.dll",
            ],
            Self::Apache => &[
                "bin/httpd.exe",
                "conf/mime.types",
                "modules/mod_authz_core.so",
                "modules/mod_dir.so",
                "modules/mod_mime.so",
            ],
            Self::Mysql => &["bin/mysqld.exe", "bin/mysql.exe"],
        }
    }

    fn valid_layout(self, folder: &Path) -> bool {
        !is_link(folder)
            && self.required_files().iter().all(|name| {
                let mut path = folder.join(name);
                if !path.is_file() {
                    return false;
                }
                while path != folder {
                    if is_link(&path) {
                        return false;
                    }
                    path.pop();
                }
                true
            })
    }

    pub fn detect_version(self, folder: &Path) -> Result<String, String> {
        if !self.valid_layout(folder) {
            return Err(format!(
                "The {} folder is missing required files. Expected: {}.",
                self.name(),
                self.required_files().join(", ")
            ));
        }
        let (exe, args, marker): (&str, &[&str], &str) = match self {
            Self::Php => ("php.exe", &["-n", "-v"], "PHP "),
            Self::Apache => ("bin/httpd.exe", &["-v"], "Apache/"),
            Self::Mysql => ("bin/mysqld.exe", &["--no-defaults", "--version"], " Ver "),
        };
        let output = Command::new(folder.join(exe))
            .args(args)
            .current_dir(folder)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| format!("Could not run {exe}: {error}"))?;
        let version = String::from_utf8_lossy(&output.stdout);
        let detected = version
            .split_once(marker)
            .and_then(|(_, tail)| tail.split_ascii_whitespace().next())
            .filter(|v| self.supports(v));
        let compatible = match self {
            Self::Php => version.contains("ZTS") && version.contains("x64"),
            Self::Apache => version.contains("Win64"),
            Self::Mysql => version.contains("Win64") && !version.contains("MariaDB"),
        };
        if let Some(detected) = detected.filter(|_| output.status.success() && compatible) {
            Ok(detected.to_owned())
        } else {
            Err(format!(
                "Could not verify the {} runtime ({exe}, {}). Check the supported x64 versions and Microsoft Visual C++ Redistributable.",
                self.name(),
                output.status
            ))
        }
    }
}

pub type Selection = std::collections::BTreeMap<Runtime, String>;

pub fn read_selection(root: &Path) -> Result<Selection, String> {
    match File::open(selection_file(root)?) {
        Ok(file) => serde_json::from_reader(file)
            .map_err(|error| format!("Could not read Volt’s runtime selections: {error}")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Selection::new()),
        Err(error) => Err(format!("Could not open Volt’s runtime selections: {error}")),
    }
}

pub fn load_selection(root: &Path) -> Result<Selection, String> {
    let mut selection = read_selection(root)?;
    let count = selection.len();
    for runtime in RUNTIMES {
        if let std::collections::btree_map::Entry::Vacant(entry) = selection.entry(runtime)
            && let Some(version) = versions(root, runtime).into_iter().next()
        {
            entry.insert(version);
        }
    }
    if selection.len() != count {
        save_selection(root, &selection)?;
    }
    Ok(selection)
}

fn selection_file(root: &Path) -> Result<PathBuf, String> {
    let parent = root
        .parent()
        .ok_or("Could not locate Volt’s config folder.")?;
    Ok(parent.join("config").join("runtime-versions.json"))
}

pub fn save_selection(root: &Path, selection: &Selection) -> Result<(), String> {
    let path = selection_file(root)?;
    let config = path
        .parent()
        .ok_or("Could not locate Volt’s config folder.")?;
    prepare(config)?;
    let mut file = tempfile::NamedTempFile::new_in(config).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, selection).map_err(|e| e.to_string())?;
    file.persist(path)
        .map_err(|error| format!("Could not save Volt’s runtime selections: {error}"))?;
    Ok(())
}

fn version_parts(version: &str) -> Option<(u16, u16, u16)> {
    let mut parts = version.split('.');
    let parse = |part: &str| {
        let value = part.parse::<u16>().ok()?;
        (value.to_string() == part).then_some(value)
    };
    let version = (
        parse(parts.next()?)?,
        parse(parts.next()?)?,
        parse(parts.next()?)?,
    );
    parts.next().is_none().then_some(version)
}

pub fn versions(root: &Path, runtime: Runtime) -> Vec<String> {
    let mut versions = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join(runtime.name())) {
        versions.extend(entries.flatten().filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (runtime.supports(&name) && runtime.installed_folder(root, &name).is_some())
                .then_some(name)
        }));
    }
    versions.sort_unstable_by_key(|version| std::cmp::Reverse(version_parts(version)));
    if runtime.installed_folder(root, LEGACY).is_some() {
        versions.insert(0, LEGACY.into());
    }
    versions
}

pub fn active_version(root: &Path, runtime: Runtime, selection: &Selection) -> Option<String> {
    selection
        .get(&runtime)
        .cloned()
        .or_else(|| versions(root, runtime).into_iter().next())
}

pub fn active_folder(root: &Path, runtime: Runtime, selection: &Selection) -> Option<PathBuf> {
    runtime.installed_folder(root, &active_version(root, runtime, selection)?)
}

pub fn status(root: &Path, selection: &Selection) -> Vec<RuntimeStatus> {
    RUNTIMES
        .into_iter()
        .map(|id| RuntimeStatus {
            id,
            folder: root.join(id.name()),
            downloads: id
                .packages()
                .iter()
                .map(|package| package.version)
                .collect(),
            versions: versions(root, id),
            active_version: active_version(root, id, selection),
            active_folder: active_folder(root, id, selection),
        })
        .collect()
}

pub fn prepare(root: &Path) -> Result<(), String> {
    if is_link(root) {
        return Err("Volt’s runtime directory must not be a link or junction.".into());
    }
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not create Volt’s runtime directory: {error}"))
}

pub fn install(
    root: &Path,
    runtime: Runtime,
    version: Option<String>,
    source: Option<String>,
) -> Result<String, String> {
    prepare(root)?;
    let parent = root.join(runtime.name());
    prepare(&parent)?;
    let package = match (version.as_deref(), &source) {
        (Some(version), None) => {
            let package = runtime
                .packages()
                .iter()
                .find(|package| package.version == version)
                .ok_or("This version isn’t in Volt’s download catalog.")?;
            destination_available(&parent.join(package.version))?;
            Some(package)
        }
        (None, Some(_)) => None,
        _ => return Err("Choose a version to download or a ZIP or folder to import.".into()),
    };
    let temp = tempfile::Builder::new()
        .prefix(".volt-install-")
        .tempdir_in(root)
        .map_err(|error| format!("Could not create a runtime staging directory: {error}"))?;
    let staging = temp.path().join("runtime");
    fs::create_dir(&staging).map_err(|error| error.to_string())?;
    let staging = dunce::canonicalize(staging).map_err(|error| error.to_string())?;
    let staged_runtime = if let Some(source) = source {
        let source = resolve_import(&source)?;
        if source.is_dir() {
            if staging.starts_with(&source)
                || dunce::canonicalize(&parent).is_ok_and(|parent| parent.starts_with(&source))
            {
                return Err(
                    "Select the extracted runtime folder, not a parent of Volt’s managed directory."
                        .into(),
                );
            }
            if !runtime.valid_layout(&source) {
                return Err(format!(
                    "Select the extracted {} folder containing {}.",
                    runtime.name(),
                    runtime.required_files().join(", ")
                ));
            }
            copy_runtime(runtime, &source, &staging)
                .map_err(|error| format!("Could not import the runtime: {error}"))?;
            staging
        } else {
            stage_import_zip(runtime, &source, &staging)?
        }
    } else {
        if let Some(package) = package {
            let archive = temp.path().join("download.zip");
            download(package, &archive)?;
            extract(&archive, &staging, package.archive_root)
                .map_err(|error| format!("Could not extract the verified ZIP: {error}"))?;
        }
        staging
    };
    let detected = runtime.detect_version(&staged_runtime)?;
    if package.is_some_and(|package| package.version != detected) {
        return Err(format!(
            "The archive reports version {detected}, which doesn’t match the catalog."
        ));
    }
    let destination = parent.join(&detected);
    destination_available(&destination)?;
    if destination.is_dir() {
        fs::remove_dir(&destination).map_err(|error| error.to_string())?;
    }
    fs::rename(staged_runtime, destination)
        .map_err(|error| format!("Could not install the staged runtime: {error}"))?;
    Ok(detected)
}

fn resolve_import(source: &str) -> Result<PathBuf, String> {
    let path = Path::new(source);
    let is_zip = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
    if !path.is_absolute() {
        return Err("Select a local ZIP file or extracted runtime folder.".into());
    }
    match dunce::canonicalize(path) {
        Ok(path) if path.is_dir() || (is_zip && path.is_file()) => Ok(path),
        _ => Err(format!(
            "Could not open a ZIP file or folder at {}.",
            path.display()
        )),
    }
}

fn stage_import_zip(runtime: Runtime, source: &Path, staging: &Path) -> Result<PathBuf, String> {
    extract(source, staging, "").map_err(|error| format!("Could not extract the ZIP: {error}"))?;
    let folder = if runtime.valid_layout(staging) {
        staging.to_path_buf()
    } else {
        // Vendor ZIPs may place the distribution one folder deep (e.g. Apache24).
        let mut candidate = None;
        for entry in fs::read_dir(staging).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() && runtime.valid_layout(&path) && candidate.replace(path).is_some() {
                return Err(
                    "This ZIP contains multiple matching runtimes. Import one at a time.".into(),
                );
            }
        }
        candidate.ok_or_else(|| {
            format!(
                "The ZIP does not contain a {} distribution with {}.",
                runtime.name(),
                runtime.required_files().join(", ")
            )
        })?
    };
    // Only move the distribution into place, never its existing MySQL config or databases.
    if runtime == Runtime::Mysql {
        remove_mysql_state(&folder)
            .map_err(|error| format!("Could not exclude existing MySQL files: {error}"))?;
    }
    Ok(folder)
}

fn destination_available(destination: &Path) -> Result<(), String> {
    match fs::symlink_metadata(destination) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(info)
            if info.is_dir()
                && !is_link(destination)
                && fs::read_dir(destination).is_ok_and(|mut entries| entries.next().is_none()) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "{} already contains files. Volt won’t overwrite them. Stop the component and move the folder aside if you want to replace it.",
            destination.display()
        )),
    }
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_attributes() & 0x400 != 0)
}

fn is_mysql_state(name: &std::ffi::OsStr) -> bool {
    ["data", "my.ini", "my.cnf"]
        .iter()
        .any(|item| name.eq_ignore_ascii_case(item))
}

fn remove_mysql_state(folder: &Path) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if is_mysql_state(&entry.file_name()) {
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn copy_runtime(runtime: Runtime, source: &Path, staging: &Path) -> io::Result<()> {
    let entries = walkdir::WalkDir::new(source)
        .min_depth(1)
        .into_iter()
        .filter_entry(|entry| {
            !(runtime == Runtime::Mysql && entry.depth() == 1 && is_mysql_state(entry.file_name()))
        });
    for entry in entries {
        let entry = entry?;
        if is_link(entry.path()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Links and junctions aren’t imported: {}",
                    entry.path().display()
                ),
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(io::Error::other)?;
        let to = staging.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir(&to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn download(package: &Package, path: &Path) -> Result<(), String> {
    let curl = crate::local::system_directory()
        .map_err(|error| error.to_string())?
        .join("curl.exe");
    let result = Command::new(curl)
        .args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "20",
            "--max-time",
            "900",
            "--max-filesize",
            "536870912",
            "--output",
        ])
        .arg(path)
        .arg(package.url)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Could not use Windows curl.exe: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "Download failed: {}. You can also import a ZIP file or extracted folder.",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    verify_hash(path, package.sha256)
}

fn verify_hash(path: &Path, expected: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let bytes = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if bytes == 0 {
            break;
        }
        hash.update(&buffer[..bytes]);
    }
    let actual = hash
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            use std::fmt::Write;
            let _ = write!(text, "{byte:02x}");
            text
        });
    if actual != expected {
        return Err(
            "The ZIP’s SHA-256 doesn’t match the pinned checksum. Volt did not install it.".into(),
        );
    }
    Ok(())
}

fn extract(archive: &Path, staging: &Path, archive_root: &str) -> io::Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(archive)?)?;
    let mut expanded_size = 0_u64;
    for index in 0..archive.len() {
        let mut item = archive.by_index(index)?;
        // Reject absolute and traversing names before enclosed_name normalizes them.
        if Path::new(item.name()?.as_ref())
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsafe ZIP entry path",
            ));
        }
        let path = item
            .enclosed_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Unsafe ZIP entry path"))?;
        let relative = if archive_root.is_empty() {
            path.as_path()
        } else if let Ok(path) = path.strip_prefix(archive_root) {
            path
        } else {
            continue;
        };
        if item.is_symlink()
            || relative.components().any(|part| {
                !matches!(part, Component::Normal(_))
                    || part.as_os_str().to_string_lossy().contains(':')
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsafe ZIP entry",
            ));
        }
        if relative.as_os_str().is_empty() {
            continue;
        }
        expanded_size = expanded_size.saturating_add(item.size());
        if expanded_size > 2 * 1024 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The ZIP exceeds the 2 GiB extraction limit",
            ));
        }
        let destination = staging.join(relative);
        if item.is_dir() {
            fs::create_dir_all(&destination)?;
        } else {
            fs::create_dir_all(destination.parent().expect("staging directory"))?;
            let mut file = File::create_new(destination)?;
            let size = item.size();
            if io::copy(&mut (&mut item).take(size + 1), &mut file)? != size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Incorrect ZIP entry size",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_to_replace_existing_files_and_skips_mysql_data_on_import() {
        let sandbox = tempfile::tempdir().unwrap();
        let source = sandbox.path().join("source");
        let staging = sandbox.path().join("staging");
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::create_dir_all(source.join("Data")).unwrap();
        fs::create_dir(&staging).unwrap();
        fs::write(source.join("Data/private"), "private").unwrap();
        fs::write(source.join("my.ini"), "local config").unwrap();
        fs::write(source.join("bin/mysql.exe"), "test").unwrap();
        copy_runtime(Runtime::Mysql, &source, &staging).unwrap();
        assert!(!staging.join("Data").exists());
        assert!(!staging.join("my.ini").exists());
        assert!(staging.join("bin/mysql.exe").is_file());
        assert!(destination_available(&staging).is_err());
    }

    fn write_zip(path: &Path, names: impl IntoIterator<Item = String>) {
        use std::io::Write;
        let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
        for name in names {
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"test").unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn imports_flat_and_wrapped_runtime_archives() {
        for (runtime, prefix) in [(Runtime::Php, ""), (Runtime::Apache, "Apache24/")] {
            let sandbox = tempfile::tempdir().unwrap();
            let archive = sandbox.path().join("runtime.ZIP");
            let staging = sandbox.path().join("staging");
            fs::create_dir(&staging).unwrap();
            write_zip(
                &archive,
                runtime
                    .required_files()
                    .iter()
                    .map(|name| format!("{prefix}{name}")),
            );
            let source = resolve_import(archive.to_str().unwrap()).unwrap();
            let folder = stage_import_zip(runtime, &source, &staging).unwrap();
            assert_eq!(folder, staging.join(prefix));
            assert!(runtime.valid_layout(&folder));
        }
    }

    #[test]
    fn excludes_mysql_data_and_configuration_from_zip_imports() {
        let sandbox = tempfile::tempdir().unwrap();
        let archive = sandbox.path().join("runtime.zip");
        let staging = sandbox.path().join("staging");
        fs::create_dir(&staging).unwrap();
        write_zip(
            &archive,
            [
                "bin/mysql.exe",
                "bin/mysqld.exe",
                "Data/private",
                "my.ini",
                "MY.CNF",
            ]
            .map(|name| format!("mysql-8.4.13-winx64/{name}")),
        );
        let folder = stage_import_zip(Runtime::Mysql, &archive, &staging).unwrap();
        assert!(Runtime::Mysql.valid_layout(&folder));
        for name in ["Data", "my.ini", "MY.CNF"] {
            assert!(!folder.join(name).exists());
        }
    }

    #[test]
    fn rejects_unsafe_or_incompatible_runtime_archives() {
        let sandbox = tempfile::tempdir().unwrap();
        let archive = sandbox.path().join("runtime.zip");
        let staging = sandbox.path().join("staging");
        fs::create_dir(&staging).unwrap();
        write_zip(&archive, ["../escaped.exe".into()]);
        let error = stage_import_zip(Runtime::Php, &archive, &staging).unwrap_err();
        assert!(error.contains("Unsafe ZIP entry path"));
        assert!(!sandbox.path().join("escaped.exe").exists());

        write_zip(&archive, ["README.txt".into()]);
        let error = stage_import_zip(Runtime::Php, &archive, &staging).unwrap_err();
        assert!(error.contains("does not contain a php distribution"));
    }

    #[test]
    fn existing_flat_install_stays_active_until_explicitly_switched() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path().join("runtimes");
        for version in [LEGACY, "8.4.6", "8.4.13"] {
            let folder = Runtime::Mysql.folder(&root, version).unwrap();
            fs::create_dir_all(folder.join("bin")).unwrap();
            for name in Runtime::Mysql.required_files() {
                fs::write(folder.join(name), "test").unwrap();
            }
        }
        assert_eq!(versions(&root, Runtime::Mysql), [LEGACY, "8.4.13", "8.4.6"]);
        let mut selection = load_selection(&root).unwrap();
        assert_eq!(selection.get(&Runtime::Mysql).unwrap(), LEGACY);
        selection.insert(Runtime::Mysql, "8.4.6".into());
        save_selection(&root, &selection).unwrap();
        assert_eq!(load_selection(&root).unwrap(), selection);
        fs::remove_dir_all(root.join("mysql/8.4.6")).unwrap();
        assert!(active_folder(&root, Runtime::Mysql, &selection).is_none());
        assert!(Runtime::Mysql.installed_folder(&root, LEGACY).is_some());
        assert!(Runtime::Mysql.folder(&root, "../8.4.6").is_none());
    }

    #[test]
    fn rejects_an_incorrect_archive_hash() {
        let sandbox = tempfile::tempdir().unwrap();
        let file = sandbox.path().join("download.zip");
        fs::write(&file, b"abc").unwrap();
        assert!(verify_hash(&file, "bad").is_err());
        verify_hash(
            &file,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
    }

    #[test]
    #[ignore = "downloads the pinned vendor archives and requires the VC++ x64 runtime"]
    fn downloads_and_validates_pinned_stack() {
        let sandbox = tempfile::tempdir().unwrap();
        let downloads = sandbox.path().join("downloads");
        std::thread::scope(|scope| {
            for runtime in RUNTIMES {
                for package in runtime.packages() {
                    let root = &downloads;
                    scope.spawn(move || {
                        install(root, runtime, Some(package.version.into()), None).unwrap_or_else(
                            |error| panic!("{runtime:?} {}: {error}", package.version),
                        );
                        assert!(runtime.installed_folder(root, package.version).is_some());
                    });
                }
            }
        });
        let imports = sandbox.path().join("imports");
        let version = install(
            &imports,
            Runtime::Php,
            None,
            Some(downloads.join("php/8.3.37").display().to_string()),
        )
        .unwrap();
        assert_eq!(version, "8.3.37");
        assert!(Runtime::Php.installed_folder(&imports, &version).is_some());
        assert_eq!(versions(&downloads, Runtime::Php), ["8.4.28", "8.3.37"]);
    }
}
