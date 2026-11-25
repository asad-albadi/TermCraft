[CmdletBinding()]
param([Parameter(Mandatory)][string]$Archive)

$ErrorActionPreference = 'Stop'
$zip = (Resolve-Path -LiteralPath $Archive).Path
$scratch = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ("TermCraft package " + [guid]::NewGuid().ToString('N')))
$local = [Environment]::GetFolderPath('LocalApplicationData')
$menu = [Environment]::GetFolderPath('Programs')
$programs = Join-Path $local 'Programs/TermCraft'
$fixtureName = 'qualification.' + [guid]::NewGuid().ToString('N')
$workspace = Join-Path $scratch.FullName 'fixture workspace'
$null = New-Item -ItemType Directory -Path $workspace
$originals = @{}
foreach ($name in 'workspace.json', 'vault.json', 'recovery.bin') {
    $path = Join-Path $workspace $name
    [IO.File]::WriteAllBytes($path, [Text.Encoding]::UTF8.GetBytes("Synthetic preservation marker $name"))
    $originals[$path] = (Get-FileHash -LiteralPath $path).Hash
}

$shell = New-Object -ComObject WScript.Shell
$installed = @()
try {
    foreach ($revision in 1, 2) {
        $expanded = Join-Path $scratch.FullName "version $revision"
        Expand-Archive -LiteralPath $zip -DestinationPath $expanded
        $root = @(Get-ChildItem -LiteralPath $expanded -Directory)
        if ($root.Count -ne 1) { throw 'Expected a single package root.' }
        $manifestPath = Join-Path $root[0].FullName 'PACKAGE-MANIFEST.json'
        $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
        $manifest.version = "0.0.$revision+$fixtureName"
        $manifest | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $manifestPath -Encoding utf8
        $version = "$($manifest.version)-$($manifest.architecture)"
        $installPath = Join-Path $programs $version
        $link = Join-Path $menu "TermCraft $version.lnk"
        if ((Test-Path -LiteralPath $installPath) -or (Test-Path -LiteralPath $link)) {
            throw 'Fixture install path is already occupied.'
        }
        $installer = Join-Path $root[0].FullName 'install-per-user.ps1'
        & $installer -Action Install
        $installed += @{ Installer = $installer; Path = $installPath; Link = $link }
        $program = Join-Path $installPath 'harbor-workspace.exe'
        $shortcut = $shell.CreateShortcut($link)
        if ($shortcut.TargetPath -ne $program -or $shortcut.WorkingDirectory -ne $installPath) {
            throw 'Start menu shortcut differs from the installed target.'
        }
        foreach ($file in 'harbor-workspace.exe', 'termcraft-rdp-engine.exe') {
            if ((Get-FileHash -LiteralPath (Join-Path $installPath $file)).Hash -ne $manifest.files.$file) {
                throw "Installed executable differs from the package: $file"
            }
        }
    }

    $first = $installed[0]
    $extra = Join-Path $first.Path 'additional-file.txt'
    [IO.File]::WriteAllText($extra, 'must survive refusal')
    try {
        $refused = $false
        try { & $first.Installer -Action Uninstall }
        catch {
            if ($_.Exception.Message -notlike '*additional file*') { throw }
            $refused = $true
        }
        if (!$refused -or [IO.File]::ReadAllText($extra) -ne 'must survive refusal') {
            throw 'Uninstall failed to preserve an additional file.'
        }
    } finally {
        if (Test-Path -LiteralPath $extra -PathType Leaf) {
            Remove-Item -LiteralPath $extra
        }
    }

    foreach ($entry in $installed) {
        & $entry.Installer -Action Uninstall
        if ((Test-Path -LiteralPath $entry.Path) -or (Test-Path -LiteralPath $entry.Link)) {
            throw 'An installed version or Start menu shortcut remained after uninstall.'
        }
    }
    foreach ($path in $originals.Keys) {
        if ((Get-FileHash -LiteralPath $path).Hash -ne $originals[$path]) {
            throw 'Installer changed a preservation fixture.'
        }
    }
    Write-Output 'Verified per-user side-by-side installation, shortcuts, extra-file refusal, and uninstall.'
} finally {
    foreach ($entry in $installed) {
        if (Test-Path -LiteralPath $entry.Path) { & $entry.Installer -Action Uninstall }
    }
    # Keep the extracted disposable fixture for diagnosis, without deleting user files.
    Write-Output "Fixture directory: $($scratch.FullName)"
}
