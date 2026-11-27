[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Archive,
    [string]$PreviousArchive,
    [string]$VaultFixture,
    [string]$SoftwareGlDirectory,
    [switch]$UsePackageVersions,
    [string]$QualificationState,
    [string]$StageRecords
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows -or $env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_ENVIRONMENT -cne 'github-hosted') {
    throw 'This test driver requires a disposable GitHub-hosted Windows runner.'
}
$zip = (Resolve-Path -LiteralPath $Archive).Path
$archives = @($zip, $zip)
if ($UsePackageVersions -and !$PreviousArchive) {
    throw 'Supply both independently versioned archives with -UsePackageVersions.'
}
if ($PreviousArchive) {
    $archives[0] = (Resolve-Path -LiteralPath $PreviousArchive).Path
}
$fixtureInput = $null
if ($QualificationState) {
    if (!$UsePackageVersions -or !$StageRecords -or !$SoftwareGlDirectory) {
        throw 'Native qualification requires both package versions, result output and pinned graphics.'
    }
    $fixtureInput = [Console]::In.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($fixtureInput)) { throw 'Missing private workspace fixture input.' }
}
$scratch = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ("TermCraft package " + [guid]::NewGuid().ToString('N')))
$local = [Environment]::GetFolderPath('LocalApplicationData')
$menu = [Environment]::GetFolderPath('Programs')
$programs = Join-Path $local 'Programs/TermCraft'
$fixtureName = 'qualification.' + [guid]::NewGuid().ToString('N')

$shell = New-Object -ComObject WScript.Shell
$installed = @()
$appHashes = @()
$archiveVersions = @()
$workingData = Join-Path $scratch.FullName 'installed workspace'
if (!$QualificationState) {
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
}
$softwareGl = @()
if ($SoftwareGlDirectory) {
    $fixture = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'windows-software-gl.json') -Raw | ConvertFrom-Json
    $softwareGl = @($fixture.files.PSObject.Properties)
    $SoftwareGlDirectory = (Resolve-Path -LiteralPath $SoftwareGlDirectory).Path
}

$records = [System.Collections.Generic.List[object]]::new()
function Record-Stage {
    param([string]$Name, [string]$Role, [hashtable]$Paths, [string]$Program)
    if (!$QualificationState) { return }
    $inputPath = Join-Path $scratch.FullName "$Name-paths.json"
    $resultPath = Join-Path $scratch.FullName "$Name-result.json"
    $Paths | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $inputPath -Encoding utf8
    $options = if ($Program) { @('--program', $Program) } else { @() }
    $fixtureInput | & python (Join-Path $PSScriptRoot '../scripts/package-native-lifecycle.py') windows-stage `
        --state $QualificationState --name $Name --role $Role --paths $inputPath --result $resultPath @options
    if ($LASTEXITCODE -ne 0) { throw "Qualification stage failed: $Name" }
    $records.Add((Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json))
}

function Assert-WorkspacePreserved {
    if ($QualificationState) { return }
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

function Start-Installed {
    param([hashtable]$entry, [string]$Role, [string]$Name)
    $installPath = $entry.Path
    $program = Join-Path $installPath 'harbor-workspace.exe'
    $shortcut = $shell.CreateShortcut($entry.Link)
    if ($shortcut.TargetPath -ne $program -or $shortcut.WorkingDirectory -ne $installPath) {
        throw 'Start menu shortcut differs from the installed target.'
    }
    $startup = Join-Path $scratch.FullName "$Name.png"
    $added = @()
    $oldDriver = $env:GALLIUM_DRIVER
    try {
        foreach ($dll in $softwareGl) {
            $inputFile = Join-Path $SoftwareGlDirectory $dll.Name
            $target = Join-Path $installPath $dll.Name
            if ((Get-FileHash -LiteralPath $inputFile).Hash -ne $dll.Value) {
                throw 'The software GL fixture DLL does not match its checksum.'
            }
            [IO.File]::Copy($inputFile, $target, $false)
            $added += $target
            if ((Get-FileHash -LiteralPath $target).Hash -ne $dll.Value) {
                throw 'The staged software GL fixture DLL does not match its checksum.'
            }
        }
        if ($softwareGl.Count) { $env:GALLIUM_DRIVER = 'llvmpipe' }
        if ($QualificationState) {
            Record-Stage $Name $Role @{
                package = $entry.Path
                registration = @{ target = $shortcut.TargetPath; working_directory = $shortcut.WorkingDirectory; shortcut = $entry.Link }
            } $shortcut.TargetPath
        } else {
            $process = Start-Process -FilePath $program -ArgumentList ('--data-dir "{0}" --screenshot "{1}"' -f $workingData,$startup) -PassThru
            if (!$process.WaitForExit(120000)) { $process.Kill(); throw 'Installed GUI timed out.' }
            $exitCode = $process.ExitCode
        }
    } finally {
        $env:GALLIUM_DRIVER = $oldDriver
        foreach ($target in $added) { Remove-Item -LiteralPath $target }
    }
    if (!$QualificationState -and ($exitCode -ne 0 -or !(Test-Path -LiteralPath $startup -PathType Leaf) -or
        (Get-Item -LiteralPath $startup).Length -lt 5000)) {
        throw 'The installed GUI did not produce its startup screenshot.'
    }
}

try {
    $plans = @()
    foreach ($revision in 1, 2) {
        $expanded = Join-Path $scratch.FullName "version $revision"
        Expand-Archive -LiteralPath $archives[$revision - 1] -DestinationPath $expanded
        $root = @(Get-ChildItem -LiteralPath $expanded -Directory)
        if ($root.Count -ne 1) { throw 'Expected a single package root.' }
        $manifestPath = Join-Path $root[0].FullName 'PACKAGE-MANIFEST.json'
        $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
        if ($UsePackageVersions) {
            $archiveVersions += [System.Management.Automation.SemanticVersion]$manifest.version
            if ($revision -eq 2 -and $archiveVersions[1] -le $archiveVersions[0]) {
                throw 'The second package version must be newer than the first.'
            }
        } else {
            $manifest.version = "0.0.$revision+$fixtureName"
            $manifest | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $manifestPath -Encoding utf8
        }
        $version = "$($manifest.version)-$($manifest.architecture)"
        $installPath = Join-Path $programs $version
        $link = Join-Path $menu "TermCraft $version.lnk"
        if ((Test-Path -LiteralPath $installPath) -or (Test-Path -LiteralPath $link)) {
            throw 'Fixture install path is already occupied.'
        }
        $installer = Join-Path $root[0].FullName 'install-per-user.ps1'
        $plans += @{ Installer = $installer; Path = $installPath; Link = $link; Manifest = $manifest }
    }
    foreach ($revision in 1, 2) {
        $plan = $plans[$revision - 1]
        $installPath = $plan.Path
        $link = $plan.Link
        $manifest = $plan.Manifest
        # Track each attempted path only after confirming both KnownFolder destinations were vacant.
        $installed += $plan
        & $plan.Installer -Action Install
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
        $role = if ($revision -eq 1) { 'baseline' } else { 'candidate' }
        Start-Installed $installed[-1] $role "windows-install-$role"
        Assert-WorkspacePreserved
    }

    if ($PreviousArchive -and $appHashes[0] -eq $appHashes[1]) {
        throw 'PreviousArchive must contain a distinct GUI executable.'
    }

    $first = $installed[0]
    $duplicateRefused = $false
    try { & $first.Installer -Action Install }
    catch {
        if ($_.Exception.Message -notlike '*already exists*') { throw }
        $duplicateRefused = $true
    }
    if (!$duplicateRefused) { throw 'Duplicate installation replaced the existing version.' }

    $notice = Join-Path $first.Path 'PRIVACY.md'
    $originalNotice = [IO.File]::ReadAllBytes($notice)
    [IO.File]::WriteAllText($notice, 'a locally modified privacy file')
    try {
        $modifiedRefused = $false
        try { & $first.Installer -Action Uninstall }
        catch {
            if ($_.Exception.Message -notlike '*file hash differs*') { throw }
            $modifiedRefused = $true
        }
        if (!$modifiedRefused -or [IO.File]::ReadAllText($notice) -ne 'a locally modified privacy file') {
            throw 'Uninstall failed to preserve a modified package file.'
        }
    } finally {
        [IO.File]::WriteAllBytes($notice, $originalNotice)
    }
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

    $second = $installed[1]
    & $first.Installer -Action Uninstall
    if ((Test-Path -LiteralPath $first.Path) -or (Test-Path -LiteralPath $first.Link) -or
        !(Test-Path -LiteralPath $second.Link) -or
        (Get-FileHash -LiteralPath (Join-Path $second.Path 'harbor-workspace.exe')).Hash -ne $appHashes[1]) {
        throw 'Removing the baseline did not preserve the installed candidate.'
    }
    Record-Stage 'windows-remove-baseline' 'baseline' @{removed = @($first.Path, $first.Link); retained = @($second.Path, $second.Link)}
    & $second.Installer -Action Uninstall
    if ((Test-Path -LiteralPath $second.Path) -or (Test-Path -LiteralPath $second.Link)) {
        throw 'The candidate or its Start menu shortcut remained after uninstall.'
    }
    Record-Stage 'windows-remove-candidate' 'candidate' @{removed = @($second.Path, $second.Link)}
    & $second.Installer -Action Install
    if ((Get-FileHash -LiteralPath (Join-Path $second.Path 'harbor-workspace.exe')).Hash -ne $appHashes[1]) {
        throw 'Reinstalled executable differs from the original package.'
    }
    Start-Installed $second 'candidate' 'windows-reinstall-candidate'
    & $second.Installer -Action Uninstall
    if ((Test-Path -LiteralPath $second.Path) -or (Test-Path -LiteralPath $second.Link)) {
        throw 'The reinstalled version or its shortcut remained after uninstall.'
    }
    Record-Stage 'windows-final-remove' 'candidate' @{removed = @($second.Path, $second.Link)}
    if ($QualificationState) {
        ConvertTo-Json -InputObject @($records.ToArray()) -Depth 20 | Set-Content -LiteralPath $StageRecords -Encoding utf8
    }
    Assert-WorkspacePreserved
    Write-Output 'Verified side-by-side installation, installed GUI startups, shortcuts, duplicate and unsafe-uninstall refusal, exact reinstall, workspace preservation, and uninstall.'
    Write-Output "Installed GUI SHA-256, in execution order: $($appHashes -join ', ')"
} finally {
    $cleanupErrors = @()
    foreach ($entry in $installed) {
        if ((Test-Path -LiteralPath $entry.Path) -or (Test-Path -LiteralPath $entry.Link)) {
            try { & $entry.Installer -Action Uninstall }
            catch { $cleanupErrors += $_.Exception.Message }
        }
    }
    Remove-Item -LiteralPath $scratch.FullName -Recurse -Force
    if ($cleanupErrors.Count) { throw "Independent installer cleanup failed: $cleanupErrors" }
}
