use crate::runtime::{self, LEGACY, RUNTIMES, Selection};
use sha2::{Digest, Sha256};
use std::{fmt::Write, io, path::Path};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use winreg::{RegKey, RegValue, enums::*, types::FromRegValue};

const SETTINGS: &str = r"Software\Volt\Installations";
const ENABLED: &str = "ManagePath";
const OWNED: &str = "PathEntries";

fn settings_key(root: &Path) -> String {
    // Separate the dev workspace and different installer destinations. Do not reuse
    // the old AppData-based PATH preference or modify PATH entries it previously added.
    let normalized = root.to_string_lossy().replace('/', "\\").to_lowercase();
    let hash = Sha256::digest(normalized.as_bytes());
    let mut key = format!(r"{SETTINGS}\");
    for byte in hash {
        let _ = write!(&mut key, "{byte:02x}");
    }
    key
}

pub fn enabled(root: &Path) -> io::Result<bool> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    match user.open_subkey(settings_key(root)) {
        Ok(settings) => Ok(read::<u32>(&settings, ENABLED)?.unwrap_or(0) != 0),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn sync(root: &Path, choice: Option<bool>, selection: &Selection) -> io::Result<bool> {
    let was_enabled = enabled(root)?;
    let enabled = choice.unwrap_or(was_enabled);
    if choice.is_none() && !enabled {
        return Ok(false);
    }
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let (settings, _) = user.create_subkey(settings_key(root))?;
    let (environment, _) = user.create_subkey("Environment")?;
    if sync_keys(root, choice, enabled, selection, &settings, &environment)? {
        notify();
    }
    Ok(enabled)
}

fn sync_keys(
    root: &Path,
    choice: Option<bool>,
    enabled: bool,
    selection: &Selection,
    settings: &RegKey,
    environment: &RegKey,
) -> io::Result<bool> {
    let previous_raw = raw_path(environment)?;
    let kind = previous_raw
        .as_ref()
        .map_or(REG_EXPAND_SZ, |value| value.vtype.clone());
    if !matches!(kind, REG_SZ | REG_EXPAND_SZ) {
        return Err(io::Error::other(
            "The Windows user PATH is not a registry string.",
        ));
    }
    let previous = previous_raw
        .as_ref()
        .map(String::from_reg_value)
        .transpose()?
        .unwrap_or_default();
    // Only manage fixed paths under this Volt installation, never external import sources.
    let mut desired = Vec::new();
    if enabled {
        for id in RUNTIMES {
            if let Some(folder) = runtime::active_folder(root, id, selection) {
                let path = id.binaries_dir(&folder);
                let path = path.to_str().filter(|path| !path.contains([';', '"', '%']))
                    .ok_or_else(|| io::Error::other("Volt’s runtime directory contains characters that cannot be safely added to PATH."))?;
                desired.push(path.to_owned());
            }
        }
    }
    let mut previous_owned = read::<Vec<String>>(settings, OWNED)?.unwrap_or_default();
    previous_owned.retain(|entry| !entry.is_empty());
    let authorized = previous_owned
        .iter()
        .filter(|entry| managed_path(root, Path::new(entry)))
        .cloned()
        .collect::<Vec<_>>();
    let (next, owned) = plan(&previous, &authorized, &desired);
    if next.encode_utf16().count() >= 32767 {
        return Err(io::Error::other(
            "The updated user PATH exceeds Windows’ size limit.",
        ));
    }
    if next != previous {
        if raw_path(environment)? != previous_raw {
            return Err(io::Error::other(
                "User PATH changed during sync. Click Rescan and try again.",
            ));
        }
        let bytes = next
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        environment.set_raw_value("Path", &RegValue { bytes, vtype: kind })?;
    }
    let settings_result: io::Result<()> = (|| {
        if owned != previous_owned {
            settings.set_value(OWNED, &owned)?;
        }
        if choice.is_some() {
            settings.set_value(ENABLED, &u32::from(enabled))?;
        }
        Ok(())
    })();
    if settings_result.is_err() {
        if next != previous {
            if let Some(previous) = previous_raw {
                let _ = environment.set_raw_value("Path", &previous);
            } else {
                let _ = environment.delete_value("Path");
            }
        }
        let _ = settings.set_value(OWNED, &previous_owned);
    }
    settings_result?;
    Ok(next != previous)
}

fn managed_path(root: &Path, path: &Path) -> bool {
    RUNTIMES.into_iter().any(|id| {
        let base = root.join(id.name());
        let folder = match id {
            runtime::Runtime::Php => path,
            _ if path.file_name().is_some_and(|name| name == "bin") => match path.parent() {
                Some(folder) => folder,
                None => return false,
            },
            _ => return false,
        };
        folder == base
            || (folder.parent() == Some(base.as_path())
                && folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|v| id.supports(v) && v != LEGACY))
    })
}

fn raw_path(key: &RegKey) -> io::Result<Option<RegValue<'static>>> {
    match key.get_raw_value("Path") {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn read<T: winreg::types::FromRegValue>(key: &RegKey, value: &str) -> io::Result<Option<T>> {
    match key.get_value(value) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn same_path(a: &str, b: &str) -> bool {
    let normalize = |part: &str| {
        part.trim()
            .trim_matches('"')
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_lowercase()
    };
    normalize(a) == normalize(b)
}

fn plan(previous: &str, previously_owned: &[String], desired: &[String]) -> (String, Vec<String>) {
    let mut paths: Vec<_> = if previous.is_empty() {
        Vec::new()
    } else {
        previous.split(';').collect()
    };
    let mut owned = Vec::new();
    for entry in previously_owned {
        if let Some(index) = paths.iter().rposition(|item| *item == entry) {
            if desired.contains(entry) {
                owned.push(entry.clone());
            } else {
                paths.remove(index);
            }
        }
    }
    for entry in desired {
        if !paths.iter().any(|item| same_path(item, entry)) {
            paths.push(entry);
            owned.push(entry.clone());
        }
    }
    (paths.join(";"), owned)
}

fn notify() {
    let message = "Environment\0".encode_utf16().collect::<Vec<_>>();
    // Tell Explorer to refresh the environment used for new child processes.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            message.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            1000,
            std::ptr::null_mut(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_scoped_to_the_installation() {
        assert_eq!(
            settings_key(Path::new(r"C:\Volt\runtimes")),
            settings_key(Path::new("c:/volt/runtimes"))
        );
        assert_ne!(
            settings_key(Path::new(r"C:\Volt\runtimes")),
            settings_key(Path::new(r"D:\Volt\runtimes"))
        );
    }

    #[test]
    fn tracks_only_added_entries_and_preserves_existing_path() {
        let old = r#"C:\Git\bin;;%OTHER%;"c:/Volt/php/";"#;
        let desired = [r"C:\Volt\php".into(), r"C:\Volt\mysql\bin".into()];
        let (path, owned) = plan(old, &[], &desired);
        assert_eq!(owned, desired[1..]);
        assert_eq!(plan(&path, &owned, &desired), (path.clone(), owned.clone()));
        assert_eq!(plan(&path, &owned, &[]), (old.into(), Vec::new()));
    }

    #[test]
    fn writes_and_restores_a_registry_fixture_without_touching_real_path() {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let sandbox = tempfile::tempdir().unwrap();
        let name = format!(
            r"Software\Volt-Test-{}",
            sandbox.path().file_name().unwrap().to_string_lossy()
        );
        let (key, _) = user.create_subkey(&name).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (environment, _) = key.create_subkey("Environment").unwrap();
            let (settings, _) = key.create_subkey("Settings").unwrap();
            let previous = RegValue {
                vtype: REG_EXPAND_SZ,
                bytes: r"%USERPROFILE%\Tools;C:\Git"
                    .encode_utf16()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            };
            environment.set_raw_value("Path", &previous).unwrap();
            let root = sandbox.path();
            let runtime = crate::runtime::Runtime::Mysql;
            let dir = runtime.binaries_dir(&runtime.folder(root, "8.4.13").unwrap());
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("mysql.exe"), "test").unwrap();
            std::fs::write(dir.join("mysqld.exe"), "test").unwrap();
            let mut selection = Selection::from([(runtime, "8.4.13".into())]);
            assert!(
                sync_keys(root, Some(true), true, &selection, &settings, &environment).unwrap()
            );
            assert_eq!(
                raw_path(&environment).unwrap().unwrap().vtype,
                REG_EXPAND_SZ
            );
            assert_eq!(
                read::<Vec<String>>(&settings, OWNED).unwrap().unwrap(),
                [dir.display().to_string()]
            );
            let other = runtime.binaries_dir(&runtime.folder(root, "8.4.6").unwrap());
            std::fs::create_dir_all(&other).unwrap();
            std::fs::write(other.join("mysql.exe"), "test").unwrap();
            std::fs::write(other.join("mysqld.exe"), "test").unwrap();
            selection.insert(runtime, "8.4.6".into());
            assert!(sync_keys(root, None, true, &selection, &settings, &environment).unwrap());
            assert!(
                !read::<String>(&environment, "Path")
                    .unwrap()
                    .unwrap()
                    .contains(&dir.display().to_string())
            );
            assert_eq!(
                read::<Vec<String>>(&settings, OWNED).unwrap().unwrap(),
                [other.display().to_string()]
            );
            assert!(
                sync_keys(
                    root,
                    Some(false),
                    false,
                    &selection,
                    &settings,
                    &environment
                )
                .unwrap()
            );
            assert_eq!(raw_path(&environment).unwrap(), Some(previous));
        }));
        drop(key);
        user.delete_subkey_all(&name).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
