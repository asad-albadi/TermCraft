[CmdletBinding()]
param([Parameter(Mandatory = $true)][ValidateSet('Install', 'Uninstall')][string]$Action)

$ErrorActionPreference = 'Stop'
$manifestPath = Join-Path $PSScriptRoot 'PACKAGE-MANIFEST.json'
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.platform -ne 'windows' -or
    $manifest.version -cnotmatch '^\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$' -or
    $manifest.architecture -cnotmatch '^(x86_64|aarch64)$') {
    throw 'Invalid TermCraft Windows package manifest.'
}
$version = "$($manifest.version)-$($manifest.architecture)"
$local = [Environment]::GetFolderPath('LocalApplicationData')
$programParent = Join-Path $local 'Programs'
$programs = Join-Path $programParent 'TermCraft'
$installDir = Join-Path $programs $version
$menu = [Environment]::GetFolderPath('Programs')
$menuName = "TermCraft $version.lnk"
$shortcutPath = Join-Path $menu $menuName
$programPath = Join-Path $installDir 'harbor-workspace.exe'
$manifestHash = (Get-FileHash -LiteralPath $manifestPath).Hash
$files = @($manifest.files.PSObject.Properties)
$expected = @('PACKAGE-MANIFEST.json') + @($files.Name)
if ('harbor-workspace.exe' -notin $expected -or 'termcraft-rdp-engine.exe' -notin $expected) {
    throw 'Package is missing the program or the RDP helper.'
}
if ($expected.Count -ne @($expected | Sort-Object -Unique).Count) {
    throw 'Package manifest contains duplicate paths.'
}

function Assert-OrdinaryItem([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Unexpected redirected path: $Path"
    }
    return $item
}

function Resolve-ManifestFile([string]$Root, [string]$Relative) {
    $path = $Root
    foreach ($part in $Relative.Split('/')) {
        if (!$part -or $part -eq '.' -or $part -eq '..' -or
            $part.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0 -or
            $part.EndsWith('.') -or $part.EndsWith(' ') -or
            $part -match '^(CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])(\.|$)') {
            throw "Invalid package path: $Relative"
        }
        $path = Join-Path $path $part
        $null = Assert-OrdinaryItem $path
    }
    if (!(Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing package file: $Relative"
    }
    return $path
}

function Assert-PackageContents([string]$Root) {
    $null = Assert-OrdinaryItem $Root
    $rootManifest = Resolve-ManifestFile $Root 'PACKAGE-MANIFEST.json'
    if ((Get-FileHash -LiteralPath $rootManifest).Hash -ne $manifestHash) {
        throw 'Installed package manifest differs from this installer.'
    }
    foreach ($file in $files) {
        $path = Resolve-ManifestFile $Root $file.Name
        if ((Get-FileHash -LiteralPath $path).Hash -ne $file.Value) {
            throw "Package file hash differs: $($file.Name)"
        }
    }
}

function Assert-OnlyInstalledFiles([string]$Root, [string]$Prefix) {
    foreach ($item in Get-ChildItem -LiteralPath $Root -Force) {
        $null = Assert-OrdinaryItem $item.FullName
        $path = "$Prefix$($item.Name)"
        if ($item.PSIsContainer) {
            if (!($expected | Where-Object { $_.StartsWith("$path/", [StringComparison]::OrdinalIgnoreCase) })) {
                throw "Move the additional directory out of the program directory first: $path"
            }
            Assert-OnlyInstalledFiles $item.FullName "$path/"
        } elseif ($path -notin $expected) {
            throw "Move the additional file out of the program directory first: $path"
        }
    }
}

if ($Action -eq 'Install') {
    Assert-PackageContents $PSScriptRoot
    $null = Assert-OrdinaryItem $local
    $null = New-Item -ItemType Directory -Path $programParent -Force
    $null = Assert-OrdinaryItem $programParent
    $null = New-Item -ItemType Directory -Path $programs -Force
    $null = Assert-OrdinaryItem $programs
    $null = Assert-OrdinaryItem $menu
    if ((Test-Path -LiteralPath $installDir) -or (Test-Path -LiteralPath $shortcutPath)) {
        throw 'This version or its Start menu shortcut already exists.'
    }
    $token = [Guid]::NewGuid().ToString('N')
    $staged = Join-Path $programs ".install-$token"
    $stagedLink = Join-Path $menu ".termcraft-$token.lnk"
    $published = $false
    try {
        $null = New-Item -ItemType Directory -Path $staged
        Copy-Item -LiteralPath $manifestPath -Destination $staged
        foreach ($file in $files) {
            $sourceFile = Resolve-ManifestFile $PSScriptRoot $file.Name
            $output = Join-Path $staged $file.Name
            $null = New-Item -ItemType Directory -Path (Split-Path $output) -Force
            Copy-Item -LiteralPath $sourceFile -Destination $output
        }
        Assert-PackageContents $staged
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($stagedLink)
        $shortcut.TargetPath = $programPath
        $shortcut.WorkingDirectory = $installDir
        $shortcut.IconLocation = "$(Join-Path $installDir 'TermCraft.ico'),0"
        $shortcut.Save()
        [IO.Directory]::Move($staged, $installDir)
        $published = $true
        [IO.File]::Move($stagedLink, $shortcutPath)
    } catch {
        if ($published -and !(Test-Path -LiteralPath $staged)) {
            [IO.Directory]::Move($installDir, $staged)
        }
        throw
    } finally {
        if (Test-Path -LiteralPath $staged) {
            Remove-Item -LiteralPath $staged -Recurse -Force
        }
        if (Test-Path -LiteralPath $stagedLink) {
            Remove-Item -LiteralPath $stagedLink -Force
        }
    }
    Write-Output "Installed $programPath"
} else {
    $null = Assert-OrdinaryItem $programs
    $null = Assert-OrdinaryItem $programParent
    $null = Assert-OrdinaryItem $local
    $null = Assert-OrdinaryItem $menu
    Assert-PackageContents $installDir
    Assert-OnlyInstalledFiles $installDir ''
    $hasLink = Test-Path -LiteralPath $shortcutPath
    if ($hasLink) {
        $null = Assert-OrdinaryItem $shortcutPath
        $shell = New-Object -ComObject WScript.Shell
        if ($shell.CreateShortcut($shortcutPath).TargetPath -ne $programPath) {
            throw 'Start menu shortcut points to a different application.'
        }
    }
    # The native app checks its owned registry references before any program files move.
    # Argument invocation is literal; no command string is evaluated. Fail closed on errors.
    $guard = Start-Process -FilePath $programPath -ArgumentList @('--windows-integration-uninstall-guard') -NoNewWindow -Wait -PassThru
    if ($guard.ExitCode -ne 0) {
        throw 'TermCraft still has integration references or could not check them. Remove owned integration using Tools before uninstalling; changed entries are preserved.'
    }
    $removed = Join-Path $programs ('.remove-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::Move($installDir, $removed)
    try {
        if ($hasLink) {
            Remove-Item -LiteralPath $shortcutPath -Force
        }
    } catch {
        [IO.Directory]::Move($removed, $installDir)
        throw
    }
    Remove-Item -LiteralPath $removed -Recurse -Force
    Write-Output "Uninstalled TermCraft $version"
}
