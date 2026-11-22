# The caller supplies the Windows system hosts path and validated .test names.
function Sync-VoltHosts([string] $Path, [string[]] $Hostnames) {
    $tempPath = $null
    try {
        $bytes = [IO.File]::ReadAllBytes($Path)
        # Latin-1 round trips every byte. Preserve the existing encoding and optional UTF-8 BOM.
        $encoding = [Text.Encoding]::GetEncoding(28591)
        $original = $encoding.GetString($bytes)
        if ($original.IndexOf([char]0) -ge 0) { return 4 }
        $bom = ''
        if ($original.StartsWith($encoding.GetString([byte[]] @(239, 187, 191)))) {
            $bom = $original.Substring(0, 3)
        }
        $text = $original.Substring($bom.Length)
        $newline = if ($text.Contains("`r`n") -or -not $text.Contains("`n")) { "`r`n" } else { "`n" }
        $managed = '(?m)^127\.0\.0\.1\t[a-z0-9][a-z0-9-]{0,49}\.test\t# Volt managed\r?(?:\n|$)'
        $text = [regex]::Replace($text, $managed, '')

        $existing = @{}
        foreach ($line in ($text -split '\r?\n')) {
            $fields = @(($line -split '#', 2)[0].Trim() -split '\s+')
            for ($i = 1; $i -lt $fields.Count; $i++) {
                $alias = $fields[$i].TrimEnd('.')
                if ($Hostnames -contains $alias) {
                    if ($fields[0] -ne '127.0.0.1') { return 3 }
                    $existing[$alias] = $true
                }
            }
        }
        foreach ($hostname in $Hostnames) {
            if (-not $existing.ContainsKey($hostname)) {
                if ($text.Length -gt 0 -and -not $text.EndsWith("`n")) { $text += $newline }
                $text += "127.0.0.1`t$hostname`t# Volt managed$newline"
            }
        }
        $updated = $bom + $text
        if ($updated -ceq $original) { return 0 }

        $candidate = [IO.Path]::Combine([IO.Path]::GetDirectoryName($Path), [IO.Path]::GetRandomFileName())
        $stream = [IO.File]::Open($candidate, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
        $tempPath = $candidate
        try {
            $output = $encoding.GetBytes($updated)
            $stream.Write($output, 0, $output.Length)
        } finally { $stream.Dispose() }
        if ($encoding.GetString([IO.File]::ReadAllBytes($Path)) -cne $original) { return 6 }
        [IO.File]::Replace($tempPath, $Path, [NullString]::Value)
        $tempPath = $null
        return 0
    } catch {
        Write-Verbose $_.Exception.Message
        return 5
    } finally {
        if ($tempPath) {
            try { [IO.File]::Delete($tempPath) } catch { Write-Verbose $_.Exception.Message }
        }
    }
}
