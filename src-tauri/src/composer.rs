use super::{Runtime, catalog, download, is_link};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const COMPOSER_DIR: &str = "volt-composer";
const PHAR: &str = "composer.phar";
const LAUNCHER: &str = "composer.bat";
const INI: &str = "composer.ini";

// Keep the project working directory and always use this distribution's PHP.
const BATCH: &str = concat!(
    "@echo off\r\n",
    "setlocal DisableDelayedExpansion\r\n",
    "if exist \"%~dp0..\\php.ini\" goto configured\r\n",
    "\"%~dp0..\\php.exe\" -c \"%~dp0composer.ini\" -d \"extension_dir=%~dp0..\\ext\" \"%~dp0composer.phar\" %*\r\n",
    "exit /b %errorlevel%\r\n",
    ":configured\r\n",
    "\"%~dp0..\\php.exe\" -c \"%~dp0..\\php.ini\" \"%~dp0composer.phar\" %*\r\n",
    "exit /b %errorlevel%\r\n",
);

pub fn composer_folder(php: &Path) -> Option<PathBuf> {
    let dir = php.join(COMPOSER_DIR);
    (!is_link(&dir)
        && [PHAR, LAUNCHER, INI].iter().all(|name| {
            let file = dir.join(name);
            file.is_file() && !is_link(&file)
        }))
    .then_some(dir)
}

pub fn install_composer(php: &Path) -> Result<(), String> {
    install(php, |path| {
        download(catalog::COMPOSER_URL, catalog::COMPOSER_SHA256, path)
    })
    .map_err(|error| format!("Could not install Composer: {error}"))
}

fn install(php: &Path, fetch: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    if !Runtime::Php.valid_layout(php) {
        return Err("Select an installed PHP version first.".into());
    }
    let destination = php.join(COMPOSER_DIR);
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(format!(
                "{} already exists. Volt won’t overwrite it.",
                destination.display()
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let temp = tempfile::Builder::new()
        .prefix(".volt-composer-")
        .tempdir_in(php)
        .map_err(|error| error.to_string())?;
    let staged = temp.path().join(COMPOSER_DIR);
    fs::create_dir(&staged).map_err(|error| error.to_string())?;
    fetch(&staged.join(PHAR))?;
    let mut ini = String::new();
    for extension in ["openssl", "zip"] {
        if php
            .join("ext")
            .join(format!("php_{extension}.dll"))
            .is_file()
        {
            ini.push_str(&format!("extension={extension}\n"));
        }
    }
    fs::write(staged.join(INI), ini).map_err(|error| error.to_string())?;
    fs::write(staged.join(LAUNCHER), BATCH).map_err(|error| error.to_string())?;
    fs::rename(staged, destination).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_composer_without_modifying_php_ini_or_replacing_existing_files() {
        let sandbox = tempfile::tempdir().unwrap();
        let php = sandbox.path();
        fs::create_dir(php.join("ext")).unwrap();
        for name in Runtime::Php.required_files() {
            fs::write(php.join(name), "fixture").unwrap();
        }
        fs::write(php.join("ext/php_openssl.dll"), "fixture").unwrap();
        fs::write(php.join("php.ini"), "existing").unwrap();

        assert!(install(php, |_| Err("download failed".into())).is_err());
        assert!(!php.join(COMPOSER_DIR).exists());
        install(php, |path| {
            fs::write(path, "phar").map_err(|error| error.to_string())
        })
        .unwrap();
        let dir = composer_folder(php).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(INI)).unwrap(),
            "extension=openssl\n"
        );
        assert_eq!(fs::read_to_string(php.join("php.ini")).unwrap(), "existing");
        assert!(
            install(php, |_| panic!(
                "should not download over an existing install"
            ))
            .is_err()
        );
        assert_eq!(fs::read_to_string(dir.join(PHAR)).unwrap(), "phar");
    }
}
