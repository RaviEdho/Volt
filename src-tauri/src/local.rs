use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn system_directory() -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buffer = [0_u16; 1024];
    // The API writes at most the supplied number of UTF-16 code units.
    let size = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if size == 0 {
        Err(io::Error::last_os_error())
    } else if size >= buffer.len() {
        Err(io::Error::other(
            "Windows system directory path is too long",
        ))
    } else {
        Ok(std::ffi::OsString::from_wide(&buffer[..size]).into())
    }
}

pub fn resolve_folder(value: &str, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(value.trim());
    if !path.is_absolute() {
        return Err(format!("Enter the full {name} folder path."));
    }
    match dunce::canonicalize(path) {
        Ok(path) if path.is_dir() => Ok(path),
        _ => Err(format!(
            "The {name} folder doesn’t exist or can’t be accessed: {}",
            path.display()
        )),
    }
}

pub fn read_tail(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let offset = file.metadata()?.len().saturating_sub(4096);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(4096).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
