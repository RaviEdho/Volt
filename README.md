# Volt

A Windows desktop app for local development. Serve multiple projects with a shared Apache, PHP, and MySQL stack. Volt can download standalone runtimes, import ZIP files or extracted folders, or use binaries already on disk.

## Run locally

Install Node.js, Rust using the MSVC toolchain, and the [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows).

```sh
npm ci
npm run tauri dev
```

## Windows installer and storage

Build the installer with `npm run tauri build`. The setup executable is generated under `src-tauri/target/release/bundle/nsis/`. Fresh installs default to `C:\Volt` for the current Windows user. If that folder isn’t writable, choose a folder your account can write to; setup does not elevate itself. On a shared computer, choose a private folder under your Windows profile if other local users shouldn’t be able to read your project data.

```text
C:\Volt\
  volt.exe
  runtimes\php\8.4.28\
  runtimes\apache\2.4.71\
  runtimes\mysql\8.4.13\
  data\mysql-versions\8.4.13\
  config\
```

Runtimes, databases, and settings live next to `volt.exe` even when you choose a different install folder. Updates and uninstall leave those folders in place. Data from the earlier AppData-based preview stays untouched. `npm run tauri dev` uses the project's ignored `.volt-dev` folder instead. Windows user PATH entries and `.test` hosts mappings are still stored by Windows.

Maintainers: see the [release guide](docs/RELEASING.md) for versioning and GitHub releases.

## Set up the environment

Open **Environment** in Volt and expand **Install / import** for PHP, Apache, or MySQL. Select a version and click **Download ZIP**, click **Import ZIP…** to select an x64 distribution you already downloaded, or click **Import folder…** to select the extracted folder containing the runtime files. Volt unpacks ZIPs automatically and runs the imported binary to detect its version, so only import distributions you trust. MySQL imports don’t install the existing `data` folder or `my.ini`/`my.cnf` files. Volt’s built-in downloads use pinned SHA-256 checksums, and Volt won’t overwrite a nonempty version folder. Older releases are provided for local compatibility testing.

To install manually, click **Open runtimes**, extract into a folder named for the exact binary version, then click **Rescan**:

| Runtime folder under `C:\Volt\runtimes` (default) | Expected layout |
| --- | --- |
| `php\<version>` | `php.exe`, `php8apache2_4.dll`, and `ext\php_pdo_mysql.dll` (PHP 8 Thread Safe) |
| `apache\<version>` | `bin\httpd.exe`, `conf\mime.types`, and `modules` (Apache 2.4) |
| `mysql\<version>` | `bin\mysqld.exe` and `bin\mysql.exe` (MySQL 8.4) |

Install the [Microsoft Visual C++ x64 Redistributable](https://aka.ms/vc14/vc_redist.x64.exe) if required by your binaries. The ZIP installer validates downloaded and imported executables before putting them in place. Rescan checks the folder layout of files you add manually. Service toggles use the chosen active versions. To reuse a distribution from Laragon, use **Import folder…**. Stop Apache before changing PHP or Apache versions, and stop MySQL before changing its version. Existing flat runtime folders appear as **Existing install (unversioned)** and remain in place.

Turn on **Add active versions to user PATH** to expose `php`, `composer`, `httpd`, `mysql`, and the other bundled CLI tools. Volt switches its PATH entries to match the active versions without changing unrelated entries or the system PATH. Turning it off removes only entries Volt added. Reopen your terminal (and its parent app if needed), then try `where.exe php`, `where.exe httpd`, and `where.exe mysql`. Earlier PATH entries can still take precedence. Use Volt to start the services; commands run directly in your terminal do not use Volt’s generated service configuration.

### Use Composer with PHP

Volt downloads a pinned, SHA-256-verified Composer 2 automatically when you download or import PHP. PHP imports need internet access for this step; Volt stages a fresh Composer bundle rather than copying an existing `volt-composer` directory. For PHP already on disk (including versions you add manually), select it and click **Environment → PHP → Install / import → Install Composer for active PHP**.

With user PATH management enabled, open a new terminal and run `composer --version`. Run Composer commands from your project folder. Without PATH management, run `volt-composer\composer.bat` by its full path inside the selected PHP install. The launcher uses that install’s `php.exe` and honors its `php.ini` if present; make sure OpenSSL is enabled in a custom `php.ini` so Composer can download over HTTPS. Volt doesn’t run `composer install` for your projects.

## Run the PHP demo

Select an installed Apache 2.4 and Thread Safe PHP 8 version in **Environment**, then turn on **Apache + PHP**. PHP runs inside Apache and has no separate switch. Once ready, click **Open demo** to view the PHP version at `http://127.0.0.1:8080/`. Turn off Apache to take the demo offline. Closing Volt normally also stops its Apache process. If another app is already using port 8080, free the port before starting Volt.

Volt generates a temporary Apache config, binds only to loopback, and does not change your existing Apache or PHP setup or register a Windows service. Startup errors and available Apache logs appear in the app.

## Serve existing projects

1. With Apache stopped, add a project in Volt. Enter its full folder path and a relative web root. Use `public` for projects that have one, or `.` only if the **entire** folder is meant to be served.
2. Click **Sync .test domains** and approve the Windows UAC prompt. Volt maps registered names to `127.0.0.1` in the Windows hosts file. It leaves unrelated entries alone and refuses to overwrite a name already mapped to another address.
3. Turn on **Apache + PHP**, then click **Open** next to a project. For example, `blog` opens at `http://blog.test:8080`. All projects run together on the same Apache and PHP instance.
4. To remove a project, stop Apache, remove it from the list, then sync again to remove its Volt-managed hosts entry. Removing a project does not delete any of its files. Hosts entries persist across app restarts.

Volt serves PHP and static files and routes missing paths to a web root’s `index.php` when present. It enables `pdo_mysql` when the PHP distribution includes `ext/php_pdo_mysql.dll`. It doesn’t read `.htaccess`, install project dependencies, or set up project databases automatically. Only register code you trust: it runs with your Windows user permissions.

## Run MySQL 8.4

1. Select an installed MySQL 8.4 version and turn on **MySQL server**. The server listens only at `127.0.0.1:3307`. Each exact version has its own data directory under `C:\Volt\data\mysql-versions\<version>`. The existing unversioned MySQL install still uses `C:\Volt\data\mysql`. A different data directory starts with its own databases and accounts; switching back restores the previous version’s data. Volt doesn’t move or upgrade databases between versions, or use Laragon’s data or option files.
2. On the first start of a data directory, expand **Temporary root password**. Copy it before closing Volt. The password is shown only after initialization; Volt doesn’t save it. MySQL requires it to be changed before the first administrative session can run other statements.
3. In PowerShell, run the bundled MySQL client (replace the folder path). Enter the temporary password at the prompt, not as a command-line argument:

   ```powershell
   & 'C:\path\to\mysql-8.4\bin\mysql.exe' --no-defaults --no-login-paths --protocol=TCP --host=127.0.0.1 --port=3307 --user=root --password --ssl-mode=REQUIRED --connect-expired-password
   ```

   ```sql
   ALTER USER USER() IDENTIFIED BY 'REPLACE_WITH_A_STRONG_ROOT_PASSWORD';
   ```

If you close Volt before saving or changing the temporary password, see [MySQL’s root password reset instructions](https://dev.mysql.com/doc/refman/8.4/en/resetting-permissions.html) and use the Volt data directory shown in the app.

Use a separate, database-scoped account for each project, not `root`. Use `127.0.0.1` as the application's database host and `3307` as the port. Turning off **MySQL server** or closing Volt normally stops its MySQL process without deleting data.

### Try the PHP counter

Run these statements in the root MySQL session. Choose a different strong password for the demo account. In `GRANT`, the qualified table name gives the app access to just this table.

```sql
CREATE DATABASE volt_counter;
CREATE TABLE volt_counter.counter (id TINYINT PRIMARY KEY, hits BIGINT NOT NULL);
INSERT INTO volt_counter.counter (id, hits) VALUES (1, 0);
CREATE USER 'volt_counter'@'127.0.0.1' IDENTIFIED BY 'REPLACE_WITH_A_STRONG_DEMO_PASSWORD';
GRANT SELECT, UPDATE ON volt_counter.counter TO 'volt_counter'@'127.0.0.1';
```

Copy `examples/mysql-counter/db.local.example.php` to `examples/mysql-counter/db.local.php` and set the demo password there. Register the `examples/mysql-counter` folder as `mysql-counter` in Volt with web root `public`, sync the domain, then start Apache and open the project. Click **Increment**, stop and restart MySQL, and reload the page. The saved count should stay the same. Keep `db.local.php` outside `public`, and don’t commit it.

For a browser-only UI preview, use `npm run dev`. To type-check and build the frontend, run `npm run build`.

To test the actual pinned downloads, run `cargo test --manifest-path src-tauri/Cargo.toml downloads_and_validates_pinned_stack -- --ignored`. To test just the PHP download and Composer launcher, run `cargo test --manifest-path src-tauri/Cargo.toml downloads_php_with_composer_and_runs_it -- --ignored`. These tests use temporary folders and don’t change your PATH.

To run the PHP/MySQL smoke test, set `VOLT_TEST_MYSQL`, `VOLT_TEST_APACHE`, and `VOLT_TEST_PHP` to the respective binary folders, free ports 3307 and 8080, then run `cargo test --manifest-path src-tauri/Cargo.toml php_counter_survives_mysql_restart -- --ignored`. The test creates and removes its own data directory.
