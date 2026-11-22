use super::*;
use crate::{apache::Apache, projects::Project};
use std::io::{Read, Write};

fn sql(base: &str, user: &str, password: &str, statement: &str) -> std::process::Output {
    let mut child = Command::new(Path::new(base).join("bin/mysql.exe"))
        .args([
            "--no-defaults",
            "--no-login-paths",
            "--protocol=TCP",
            "--host=127.0.0.1",
            "--port=3307",
            "--ssl-mode=REQUIRED",
            "--connect-expired-password",
            "--user",
            user,
        ])
        .env("MYSQL_PWD", password)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(statement.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn request(cookie: &str, form: Option<&str>) -> String {
    let mut stream = TcpStream::connect("127.0.0.1:8080").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let (method, body) = form.map_or(("GET", ""), |body| ("POST", body));
    write!(stream,
        "{method} / HTTP/1.0\r\nHost: mysql-counter.test:8080\r\nConnection: close\r\nCookie: {cookie}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}", body.len(),
    ).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
#[ignore = "requires VOLT_TEST_MYSQL, VOLT_TEST_APACHE, VOLT_TEST_PHP and free loopback ports 3307/8080"]
fn php_counter_survives_mysql_restart() {
    let mysql_dir = std::env::var("VOLT_TEST_MYSQL").unwrap();
    let apache_dir = std::env::var("VOLT_TEST_APACHE").unwrap();
    let php_dir = std::env::var("VOLT_TEST_PHP").unwrap();
    let sandbox = tempfile::Builder::new()
        .prefix("Volt's smoke test ")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let storage = sandbox.path().join(r"data\mysql");
    let folder = sandbox.path().join("counter");
    fs::create_dir_all(folder.join("public")).unwrap();
    fs::write(
        folder.join("public/index.php"),
        include_str!("../../examples/mysql-counter/public/index.php"),
    )
    .unwrap();
    fs::write(
        folder.join("db.local.php"),
        "<?php return ['user' => 'volt_counter', 'password' => 'Volt_Test_only-987'];",
    )
    .unwrap();

    let mut mysql = MySql::default();
    let started = mysql.start(&storage, &mysql_dir);
    assert!(
        started.status.ready,
        "{:?}\n{}",
        started.status.error, started.status.logs
    );
    let password = started
        .temporary_password
        .expect("first start must issue a root password");
    assert!(!started.status.logs.contains(&password));
    let blocked_storage = sandbox.path().join("blocked-mysql");
    let blocked = MySql::default().start(&blocked_storage, &mysql_dir);
    assert!(
        blocked
            .status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("already in use"))
    );
    assert!(!blocked_storage.exists());
    let setup = sql(
        &mysql_dir,
        "root",
        &password,
        "ALTER USER USER() IDENTIFIED BY 'Volt_Test_root-only-987';\n\
        CREATE DATABASE volt_counter;\n\
        CREATE TABLE volt_counter.counter (id TINYINT PRIMARY KEY, hits BIGINT NOT NULL);\n\
        INSERT INTO volt_counter.counter (id, hits) VALUES (1, 0);\n\
        CREATE USER 'volt_counter'@'127.0.0.1' IDENTIFIED BY 'Volt_Test_only-987';\n\
        GRANT SELECT, UPDATE ON volt_counter.counter TO 'volt_counter'@'127.0.0.1';\n",
    );
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let denied = sql(
        &mysql_dir,
        "volt_counter",
        "Volt_Test_only-987",
        "CREATE DATABASE should_be_denied;\n",
    );
    assert!(
        !denied.status.success()
            && String::from_utf8_lossy(&denied.stderr).contains("Access denied")
    );

    let mut apache = Apache::default();
    apache.start(
        &apache_dir,
        &php_dir,
        &[Project {
            slug: "mysql-counter".into(),
            folder: folder.display().to_string(),
            web_root: "public".into(),
        }],
    );
    let initial = request("", None);
    assert!(
        initial.contains("Saved count: <strong>0</strong>"),
        "{initial}"
    );
    let cookie = initial
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let token = initial
        .split_once("name=\"token\" value=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    assert!(request(cookie, Some(&format!("token={token}"))).contains("303 See Other"));
    assert!(request(cookie, None).contains("Saved count: <strong>1</strong>"));

    let stopped = mysql.stop(&storage);
    assert!(
        !stopped.running && stopped.logs.contains("Shutdown complete"),
        "{:?}\n{}",
        stopped.error,
        stopped.logs
    );
    assert!(!accepts_connections());
    // Use a fresh controller to show that the state lives in the data folder, not the app state.
    let mut mysql = MySql::default();
    let restarted = mysql.start(&storage, &mysql_dir);
    assert!(
        restarted.status.ready && restarted.temporary_password.is_none(),
        "{:?}\n{}",
        restarted.status.error,
        restarted.status.logs
    );
    assert!(request(cookie, None).contains("Saved count: <strong>1</strong>"));
    apache.stop();
    drop(mysql);
    assert!(!accepts_connections());
}
