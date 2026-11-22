# Working on Volt

Volt is a Windows desktop app for local PHP development, built with Tauri 2, Rust, React, TypeScript, and Vite. It installs and switches standalone Apache, PHP, and MySQL runtimes and serves existing projects without moving their source. Read `README.md` for the user workflow.

## Where to look

- `src/App.tsx`: dashboard, Apache switch, and saved projects. `src/EnvironmentPanel.tsx`: runtime versions, downloads/imports, and the PATH setting. `src/MySqlPanel.tsx`: MySQL switch, logs, data path, and first-start credentials. `src/ServiceSwitch.tsx` and `src/App.css`: shared controls and styles.
- `src-tauri/src/lib.rs`: Tauri commands and service state. Keep process, filesystem, registry, and elevation work in Rust; keep long-running operations off the UI thread. If an IPC command changes, update its frontend caller and keep capabilities in `src-tauri/capabilities/` narrowly scoped.
- `src-tauri/src/runtime.rs` and `runtime_catalog.rs`: runtime discovery, active selections, imports, ZIP verification, and pinned vendor downloads. `layout.rs`: install-relative paths. `user_path.rs`: Windows user PATH ownership.
- `src-tauri/src/apache.rs`: Apache/PHP lifecycle, generated config, and virtual hosts. `projects.rs`: project names and web roots. `hosts.rs` and `sync_hosts.ps1`: explicit, elevated `.test` domain sync. `mysql.rs`: isolated MySQL lifecycle and data. `examples/mysql-counter/`: optional integration demo, not bundled in the installer.
- `src-tauri/tauri.conf.json` and `src-tauri/installer/installer.nsi`: Windows bundle and per-user setup. `.github/workflows/`: Windows CI and release automation.

## Behavior to preserve

- One active version per runtime, shared by all projects. Apache uses the active Thread Safe PHP 8 build; PHP has no separate switch. Stop Apache before switching Apache/PHP, and stop MySQL before switching MySQL. Keep primary switches and active-version dropdowns visible; put secondary actions behind disclosures.
- Services bind only to `127.0.0.1` (Apache `8080`, MySQL `3307`). Manage only processes Volt starts; do not register Windows services. Apache should serve only the selected web root, not the parent project or hidden files. Use `public` for frameworks such as Laravel.
- Fresh installs default to `C:\Volt`; packaged storage follows `volt.exe`. Dev mode uses the ignored `.volt-dev/` folder. Runtime binaries live under `runtimes/<component>/<version>`, settings under `config/`, and each MySQL version uses `data/mysql-versions/<version>`. The existing flat MySQL install uses `data/mysql`. Preserve existing installs and data; don't silently migrate, overwrite, or delete them.
- Verify built-in downloads against pinned SHA-256 hashes before extraction. Treat imported binaries as executable code and archives/paths as untrusted input. Stage installs and validate layout and version before moving files into place. Never adopt or copy existing MySQL databases as part of a runtime import.
- PATH management is opt-in and user-scoped: expose the active managed versions and remove only entries Volt added. Hosts sync is a separate, explicit UAC action; preserve unrelated entries and reject conflicting mappings. Don't edit the real hosts file, user PATH, or installed runtimes just to test a change.

## Build and test

Use Windows with Node.js 22, stable Rust with the MSVC toolchain, and the Tauri Windows prerequisites from `README.md`. Run from the repository root:

```sh
npm ci
npm run tauri dev
npm run build
npm run version:check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
powershell.exe -NoProfile -File src-tauri/tests/sync_hosts.ps1
```

`npm run dev` is a browser-only UI preview; native commands require `npm run tauri dev` or the installed app. The ordinary Rust tests use fixtures. The ignored vendor ZIP and PHP/MySQL smoke tests need network access or local binaries as documented in `README.md`; the latter also needs free loopback ports. Use temporary test data. Keep credentials, project data, downloaded runtimes, and generated build output out of Git.

## Releases and Git

- A push to `main` or a pull request runs CI without requiring a version bump. A pushed `vX.Y.Z` tag triggers the release workflow for that tagged commit. The workflow currently publishes the x64 NSIS installer and checksum after a successful build; `.github/workflows/release.yml` is the source of truth for current publishing behavior.
- For an intentional release, use `npm run version:set -- X.Y.Z` to update the npm, Cargo, and Tauri versions plus lockfiles. Check the version, commit the intended changes, and push a matching tag. See `docs/RELEASING.md` for the command sequence. To build locally without publishing, run `npm run tauri build`.
- Check `git status` before editing and preserve existing user changes. Don't commit, push, tag, publish, change repository settings, or run the installer unless asked.
