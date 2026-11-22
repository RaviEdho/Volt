use base64::{Engine, engine::general_purpose::STANDARD};
use std::{env, os::windows::process::CommandExt, path::PathBuf, process::Command};

const SCRIPT: &str = include_str!("sync_hosts.ps1");

pub fn sync(slugs: &[String]) -> Result<(), String> {
    // Only validated DNS labels go into the elevated script. Never elevate a project path or file.
    let names = crate::projects::hosts(slugs)?
        .iter()
        .map(|host| format!("'{host}'"))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "{SCRIPT}\nexit (Sync-VoltHosts ([IO.Path]::Combine([Environment]::SystemDirectory, 'drivers\\etc\\hosts')) @({names}))"
    );
    let encoded = STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let system_root =
        env::var_os("SystemRoot").ok_or("Windows system directory is unavailable.")?;
    let powershell =
        PathBuf::from(system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let launch = format!(
        r#"
        try {{
            $child = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -Verb RunAs -WorkingDirectory $PSHOME -WindowStyle Hidden -Wait -PassThru -ErrorAction Stop -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand','{encoded}')
            exit $child.ExitCode
        }} catch {{ exit 7 }}
    "#
    );
    let result = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", &launch])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW, except for the Windows UAC prompt.
        .status()
        .map_err(|error| format!("Could not start Windows PowerShell: {error}"))?;
    match result.code() {
        Some(0) => Ok(()),
        Some(3) => Err("A .test name already points somewhere else in the hosts file. Choose a different project name or resolve the conflicting entry before syncing.".into()),
        Some(4) => Err("The Windows hosts file uses an unsupported encoding. Volt did not change it.".into()),
        Some(5) => Err("Windows could not update the hosts file. Check its permissions or whether security software blocked the change.".into()),
        Some(6) => Err("The Windows hosts file changed during sync. Try again.".into()),
        Some(7) => Err("Administrator approval was declined or Windows could not show the prompt. No domains were synced.".into()),
        _ => Err(format!("Hosts file sync failed ({result}).")),
    }
}
