$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../src/sync_hosts.ps1')

function Assert-Equal($Expected, $Actual) {
    if ($Expected -cne $Actual) { throw "Expected [$Expected] but got [$Actual]" }
}

$directory = Join-Path ([IO.Path]::GetTempPath()) ('volt-hosts-test-' + [guid]::NewGuid().ToString())
$null = New-Item $directory -ItemType Directory
$fixture = Join-Path $directory 'hosts'
$encoding = [Text.Encoding]::GetEncoding(28591)
try {
    $bom = $encoding.GetString([byte[]] @(239, 187, 191))
    $manual = $bom + "# $([char]233) user settings`r`n127.0.0.1 manual.test alias.test # user mapping`r`n"
    $old = "127.0.0.1`told.test`t# Volt managed`r`n"
    [IO.File]::WriteAllBytes($fixture, $encoding.GetBytes($manual + $old))
    Assert-Equal 0 (Sync-VoltHosts $fixture @('manual.test', 'new.test'))
    $expected = $manual + "127.0.0.1`tnew.test`t# Volt managed`r`n"
    Assert-Equal $expected ($encoding.GetString([IO.File]::ReadAllBytes($fixture)))
    Assert-Equal 0 (Sync-VoltHosts $fixture @('manual.test', 'new.test'))
    Assert-Equal $expected ($encoding.GetString([IO.File]::ReadAllBytes($fixture)))
    Assert-Equal 0 (Sync-VoltHosts $fixture @())
    Assert-Equal $manual ($encoding.GetString([IO.File]::ReadAllBytes($fixture)))

    $unchanged = "192.0.2.10 other.test ClAsH.TeSt. # keep`n" + $old
    [IO.File]::WriteAllBytes($fixture, $encoding.GetBytes($unchanged))
    Assert-Equal 3 (Sync-VoltHosts $fixture @('clash.test', 'good.test'))
    Assert-Equal $unchanged ($encoding.GetString([IO.File]::ReadAllBytes($fixture)))

    $lf = "# existing`n127.0.0.1 untouched.test"
    [IO.File]::WriteAllBytes($fixture, $encoding.GetBytes($lf))
    Assert-Equal 0 (Sync-VoltHosts $fixture @('good.test'))
    Assert-Equal "$lf`n127.0.0.1`tgood.test`t# Volt managed`n" ($encoding.GetString([IO.File]::ReadAllBytes($fixture)))
    Write-Output 'Hosts fixture tests passed.'
} finally { Remove-Item -LiteralPath $directory -Recurse -Force }
