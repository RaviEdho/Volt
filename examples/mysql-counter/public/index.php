<?php

declare(strict_types=1);

header('Cache-Control: no-store');
$configPath = dirname(__DIR__) . '/db.local.php';
if (!is_file($configPath)) {
    http_response_code(503);
    exit('Copy db.local.example.php to db.local.php and set the application password. See the Volt README.');
}
if (!extension_loaded('pdo_mysql')) {
    http_response_code(503);
    exit('pdo_mysql is not loaded. Use a PHP build with php_pdo_mysql.dll and restart Apache in Volt.');
}

session_start(['cookie_httponly' => true, 'cookie_samesite' => 'Strict']);
$_SESSION['counter_token'] ??= bin2hex(random_bytes(32));
try {
    $config = require $configPath;
    $db = new PDO(
        'mysql:host=127.0.0.1;port=3307;dbname=volt_counter;charset=utf8mb4',
        $config['user'],
        $config['password'],
        [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION],
    );
    if ($_SERVER['REQUEST_METHOD'] === 'POST') {
        $token = $_POST['token'] ?? null;
        if (!is_string($token) || !hash_equals($_SESSION['counter_token'], $token)) {
            http_response_code(403);
            exit('Invalid form token. Reload the page and try again.');
        }
        $db->exec('UPDATE counter SET hits = hits + 1 WHERE id = 1');
        header('Location: /', true, 303);
        exit;
    }
    $hits = $db->query('SELECT hits FROM counter WHERE id = 1')->fetchColumn();
    if ($hits === false) {
        throw new RuntimeException('The counter row is missing.');
    }
} catch (Throwable $error) {
    http_response_code(503);
    exit('Could not read or write the counter. Check that Volt MySQL is running, the demo database is set up, and db.local.php uses the application credentials.');
}
?>
<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>Volt MySQL counter</title>
<h1>Volt MySQL counter</h1>
<p>Saved count: <strong><?= (int) $hits ?></strong></p>
<form method="post">
    <input type="hidden" name="token" value="<?= $_SESSION['counter_token'] ?>">
    <button type="submit">Increment</button>
</form>
</html>
