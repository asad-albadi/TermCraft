[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Archive,
    [string]$PreviousArchive,
    [string]$VaultFixture
)

$ErrorActionPreference = 'Stop'
$zip = (Resolve-Path -LiteralPath $Archive).Path
$archives = @($zip, $zip)
if ($PreviousArchive) {
    $archives[0] = (Resolve-Path -LiteralPath $PreviousArchive).Path
}
$scratch = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ("TermCraft package " + [guid]::NewGuid().ToString('N')))
$local = [Environment]::GetFolderPath('LocalApplicationData')
$menu = [Environment]::GetFolderPath('Programs')
$programs = Join-Path $local 'Programs/TermCraft'
$fixtureName = 'qualification.' + [guid]::NewGuid().ToString('N')

$shell = New-Object -ComObject WScript.Shell
$installed = @()
$appHashes = @()
$workingData = Join-Path $scratch.FullName 'installed workspace'
$null = New-Item -ItemType Directory -Path $workingData
$config = Join-Path $workingData 'workspace.json'
$userFile = Join-Path $workingData 'unrelated.txt'
[IO.File]::WriteAllText($config, '{"sessions":[{"id":"upgrade-fixture","name":"Keep this session","protocol":"SSH","host":"example.test","port":22}],"preferences":{"appearance":"Dark"}}')
[IO.File]::WriteAllText($userFile, 'keep this file across installs and uninstall')
$userHash = (Get-FileHash -LiteralPath $userFile).Hash
$vaultPath = Join-Path $workingData 'credentials.vault'
$vaultHash = $null
if ($VaultFixture) {
    Copy-Item -LiteralPath (Resolve-Path -LiteralPath $VaultFixture).Path -Destination $vaultPath
    $vaultHash = (Get-FileHash -LiteralPath $vaultPath).Hash
}

function Assert-WorkspacePreserved {
    $workspace = Get-Content -LiteralPath $config -Raw | ConvertFrom-Json
    $session = @($workspace.sessions)
    if ($session.Count -ne 1 -or $session[0].id -cne 'upgrade-fixture' -or
        $session[0].name -cne 'Keep this session' -or $session[0].protocol -cne 'SSH' -or
        $session[0].host -cne 'example.test' -or $session[0].port -ne 22 -or
        $workspace.preferences.appearance -cne 'Dark' -or
        (Get-FileHash -LiteralPath $userFile).Hash -ne $userHash) {
        throw 'The package lifecycle changed the shared workspace fixture.'
    }
    if ($vaultHash -and (Get-FileHash -LiteralPath $vaultPath).Hash -ne $vaultHash) {
        throw 'The package lifecycle changed the encrypted vault fixture.'
    }
}

try {
    foreach ($revision in 1, 2) {
        $expanded = Join-Path $scratch.FullName "version $revision"
        Expand-Archive -LiteralPath $archives[$revision - 1] -DestinationPath $expanded
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
        $appHashes += (Get-FileHash -LiteralPath $program).Hash
        $startup = Join-Path $scratch.FullName "installed-startup-$revision.png"
        & $program --data-dir $workingData --screenshot $startup
        if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $startup -PathType Leaf) -or
            (Get-Item -LiteralPath $startup).Length -lt 5000) {
            throw 'The installed GUI did not produce its startup screenshot.'
        }
        Assert-WorkspacePreserved
    }
    if ($PreviousArchive -and $appHashes[0] -eq $appHashes[1]) {
        throw 'PreviousArchive must contain a distinct GUI executable.'
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
    Assert-WorkspacePreserved
    Write-Output 'Verified per-user side-by-side installation, both installed GUI startups, shortcuts, extra-file refusal, workspace preservation, and uninstall.'
    Write-Output "Installed GUI SHA-256, in execution order: $($appHashes -join ', ')"
} finally {
    foreach ($entry in $installed) {
        if (Test-Path -LiteralPath $entry.Path) { & $entry.Installer -Action Uninstall }
    }
    # Keep the extracted disposable fixture for diagnosis, without deleting user files.
    Write-Output "Fixture directory: $($scratch.FullName)"
}
