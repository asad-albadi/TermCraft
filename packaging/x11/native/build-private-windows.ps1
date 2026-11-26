#Requires -Version 7.0
<#
.SYNOPSIS
Builds and stages an unqualified Windows x64 VcXsrv development candidate.

.EXAMPLE
From an x64 Visual Studio 2022 Developer PowerShell running PowerShell 7:
./build-private-windows.ps1 -SourceArchive C:/offline/vcxsrv.tar.gz -BuildDirectory C:/tc-x11-build -PythonExecutable C:/tools/x11-python/Scripts/python.exe -GitUnixBin 'C:/Program Files/Git/usr/bin' -ToolDirectories @('C:/tools/perl/perl/bin','C:/tools/jom','C:/tools/nasm','C:/tools/winflexbison','C:/tools/gperf/usr/bin')

.DESCRIPTION
Use an unused build directory without spaces. Supply the offline source archive
from source-contract.json, a Python environment with lxml, Mako, PyYAML, Meson
and Ninja, and the directories containing the native build tools. The script
never runs the upstream installer or publishes its output.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SourceArchive,
    [Parameter(Mandatory)][string]$BuildDirectory,
    [Parameter(Mandatory)][string]$PythonExecutable,
    [Parameter(Mandatory)][string]$GitUnixBin,
    [Parameter(Mandatory)][string[]]$ToolDirectories,
    [ValidateRange(1, 8)][int]$Jobs = 4
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows -or ![Environment]::Is64BitProcess -or
    $env:VisualStudioVersion -ne '17.0' -or $env:VSCMD_ARG_TGT_ARCH -ne 'x64' -or
    !$env:VCToolsInstallDir -or !$env:VCToolsRedistDir -or !$env:WindowsSDKVersion) {
    throw 'Use a Windows x64 Visual Studio 2022 Developer PowerShell.'
}

function Invoke-BuildTool([string]$Executable, [string[]]$Arguments) {
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Build command failed ($LASTEXITCODE): $Executable $($Arguments -join ' ')"
    }
}

function Get-Sha256([string]$File) {
    (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant()
}

$destination = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($BuildDirectory)
if ($destination -match '\s' -or (Test-Path -LiteralPath $destination)) {
    throw 'The build directory must be unused and must not contain whitespace.'
}
$archive = (Resolve-Path -LiteralPath $SourceArchive).Path
$python = (Resolve-Path -LiteralPath $PythonExecutable).Path
if ($python -match '\s') {
    throw 'Use a Python environment without spaces in its path for upstream mhmake recipes.'
}
$git = (Resolve-Path -LiteralPath $GitUnixBin).Path
$nativeDirectories = @($ToolDirectories | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })
$compiler = Join-Path $env:VCToolsInstallDir 'bin/Hostx64/x64'
$crt = Join-Path $env:VCToolsRedistDir 'x64/Microsoft.VC143.CRT'
$windowsSdk = $env:WindowsSDKVersion.TrimEnd('\')
$msbuild = (Get-Command msbuild.exe -CommandType Application | Select-Object -First 1).Path
$contractPath = Join-Path $PSScriptRoot 'source-contract.json'
$contractHash = Get-Sha256 $contractPath
$buildScriptHash = Get-Sha256 $PSCommandPath

$variables = @('PATH', 'IS64', 'CFLAGS', 'CL', 'MHMAKECONF', 'PYTHON3')
$previousEnvironment = @{}
foreach ($name in $variables) {
    $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name)
}

try {
    # Git ships its own link.exe. Select the MSVC linker before the GNU tools.
    # Select Git Bash before any Windows Subsystem for Linux stubs on PATH.
    $env:PATH = (@($compiler, (Split-Path -Parent $python)) + $nativeDirectories + @($git, $env:PATH)) -join ';'
    $programs = [ordered]@{}
    foreach ($name in @('cl.exe', 'link.exe', 'dumpbin.exe', 'perl.exe', 'jom.exe', 'nmake.exe', 'nasm.exe',
                        'win_bison.exe', 'win_flex.exe', 'gperf.exe', 'bash.exe', 'patch.exe', 'meson.exe',
                        'ninja.exe')) {
        $programs[$name] = (Get-Command $name -CommandType Application | Select-Object -First 1).Path
    }
    if ($programs['link.exe'] -ne (Join-Path $compiler 'link.exe') -or
        $programs['bash.exe'] -ne (Join-Path $git 'bash.exe') -or
        $programs['patch.exe'] -ne (Join-Path $git 'patch.exe')) {
        throw 'The build requires MSVC link.exe and the selected Git for Windows bash.exe and patch.exe.'
    }
    $pythonPackages = Invoke-BuildTool $python @(
        '-c', "import importlib.metadata as m, json; print(json.dumps({p:m.version(p) for p in ('lxml','Mako','PyYAML','meson','ninja')}))"
    ) | ConvertFrom-Json
    $pythonVersion = Invoke-BuildTool $python @('--version')

    $null = New-Item -ItemType Directory -Path $destination
    $source = Join-Path $destination 'source'
    Invoke-BuildTool $python @((Join-Path $PSScriptRoot 'prepare-source.py'), '--platform', 'windows',
        '--source-archive', $archive, '--destination', $source)
    $manifest = Join-Path $source 'PREPARED-SOURCE.json'
    $prepared = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
    if ((Get-Sha256 $contractPath) -ne $contractHash) {
        throw 'The native source contract changed during source preparation.'
    }

    # Adjust the prepared build copy only. These locations come from the
    # pinned upstream recipe. A changed recipe requires an explicit update.
    $rewrites = [ordered]@{
        'mesalib/src/compiler/makefile.srcs' = @{'c:/winflexbison/' = 2}
        'mesalib/src/mesa/makefile.srcs' = @{'c:/winflexbison/' = 2}
        'tools/mhmake/makebison.bat' = @{'c:\winflexbison\' = 1; 'c:\Python39\python.exe' = 1}
        'tools/mhmake/makelex.bat' = @{'c:\winflexbison\' = 1; 'c:\Python39\python.exe' = 1}
        'xkbcomp/bison.bat' = @{'c:\winflexbison\' = 1}
        'xorg-server/hw/xwin/doflexbison.bat' = @{'c:\winflexbison\' = 2}
        'xorg-server/dix/makefile' = @{'wsl ./generate-atoms' = 1}
        'xorg-server/xkeyboard-config/makefile' = @{'wsl ./build.sh' = 1}
    }
    $replacements = @{
        'c:/winflexbison/' = ''
        'c:\winflexbison\' = ''
        'c:\Python39\python.exe' = 'python.exe'
        'wsl ./generate-atoms' = 'bash ./generate-atoms'
        'wsl ./build.sh' = 'bash ./build.sh'
    }
    $adaptations = [ordered]@{}
    foreach ($relative in $rewrites.Keys) {
        $file = Join-Path $source $relative
        $before = Get-Sha256 $file
        $content = [IO.File]::ReadAllText($file)
        foreach ($old in $rewrites[$relative].Keys) {
            if ([regex]::Matches($content, [regex]::Escape($old)).Count -ne $rewrites[$relative][$old]) {
                throw "Unexpected upstream build recipe at $relative"
            }
            $content = $content.Replace($old, $replacements[$old])
        }
        [IO.File]::WriteAllText($file, $content)
        $adaptations[$relative] = @{prepared = $before; adapted = Get-Sha256 $file}
    }

    $env:IS64 = '1'
    $env:CFLAGS = '-FS'
    $env:CL = '/FS'
    $env:MHMAKECONF = $source
    $env:PYTHON3 = $python
    $toolHashes = [ordered]@{'python.exe' = Get-Sha256 $python; 'msbuild.exe' = Get-Sha256 $msbuild}
    foreach ($name in $programs.Keys) { $toolHashes[$name] = Get-Sha256 $programs[$name] }
    [ordered]@{
        status = 'unqualified development build'
        source_contract_sha256 = $contractHash
        source_archive_sha256 = $prepared.source_archive_sha256
        source_patch_sha256 = $prepared.patch_sha256
        prepared_source_manifest_sha256 = Get-Sha256 $manifest
        build_script_sha256 = $buildScriptHash
        windows_sdk = $windowsSdk
        msvc = $env:VCToolsVersion
        python = $pythonVersion
        python_packages = $pythonPackages
        tool_sha256 = $toolHashes
        build_adaptations = $adaptations
        jobs = $Jobs
        compiler_flags = @{CFLAGS = $env:CFLAGS; CL = $env:CL}
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $destination 'build-inputs.json') -Encoding utf8

    Push-Location $source
    try {
        Invoke-BuildTool $msbuild @('freetype/MSBuild.sln', '-t:Build', '-p:Configuration=Release',
            '-p:Platform=x64', "-p:WindowsTargetPlatformVersion=$windowsSdk", "-m:$Jobs")
        $null = New-Item -ItemType Directory -Path 'openssl/release64'
        Push-Location 'openssl/release64'
        try {
            Invoke-BuildTool $programs['perl.exe'] @('../Configure', 'VC-WIN64A', '--release')
            Invoke-BuildTool $programs['jom.exe'] @('/J', "$Jobs")
        } finally { Pop-Location }
        Push-Location 'pthreads'
        try {
            # IS64 supplies the 64 suffix already. Keep PTW32_VER at 3.
            Invoke-BuildTool $programs['nmake.exe'] @('VC-static', 'EXTRAVERSION=')
        } finally { Pop-Location }
        Invoke-BuildTool $msbuild @('tools/mhmake/mhmakevc10.sln', '-t:Build', '-p:Configuration=Release',
            '-p:Platform=x64', "-p:WindowsTargetPlatformVersion=$windowsSdk", "-m:$Jobs")

        # Generate versioned XKB data directly with native Meson before mhmake.
        # This avoids the upstream script's dependence on WSL path translation.
        $xkbBuild = Join-Path $destination 'xkb-build'
        Invoke-BuildTool $programs['meson.exe'] @('setup', $xkbBuild,
            (Join-Path $source 'xorg-server/xkeyboard-config'), '-Dxorg-rules-copy=true',
            '--prefix', (Join-Path $source 'xorg-server/xkbdata'))
        Invoke-BuildTool $programs['meson.exe'] @('compile', '-C', $xkbBuild, '-j', "$Jobs")
        Invoke-BuildTool $programs['meson.exe'] @('install', '-C', $xkbBuild, '--no-rebuild')
        Invoke-BuildTool (Join-Path $source 'tools/mhmake/Release64/mhmake.exe') @(
            "-P$Jobs", '-C', 'xorg-server', 'MAKESERVER=1'
        )
    } finally { Pop-Location }

    $candidate = Join-Path $destination 'x11/windows-x86_64-private1'
    if ((Get-Sha256 $contractPath) -ne $contractHash) {
        throw 'The native source contract changed during compilation.'
    }
    $stage = @{SourceDirectory = $source; CandidateDirectory = $candidate; CrtDirectory = $crt}
    $inventory = & (Join-Path $PSScriptRoot 'stage-private-windows.ps1') @stage
    if (($inventory | ConvertFrom-Json).source_contract_sha256 -ne $contractHash) {
        throw 'The staged candidate inventory differs from the captured native source contract.'
    }
    $inventory | Set-Content -LiteralPath (Join-Path $destination 'candidate-inventory.json') -Encoding utf8
    Push-Location $candidate
    try {
        $binaries = @(Get-ChildItem -File | Where-Object Extension -In @('.exe', '.dll') | Sort-Object Name)
        $known = [Collections.Generic.HashSet[string]]::new([string[]]$binaries.Name, [StringComparer]::OrdinalIgnoreCase)
        $system = Join-Path $env:SystemRoot 'System32'
        $imports = foreach ($binary in $binaries) {
            Invoke-BuildTool $programs['dumpbin.exe'] @('/NOLOGO', '/DEPENDENTS', $binary.Name)
        }
        $imports | Set-Content -LiteralPath (Join-Path $destination 'pe-imports.txt') -Encoding utf8
        $dependencyCount = 0
        foreach ($line in $imports) {
            if ($line -match '^\s{4}([a-zA-Z0-9._-]+\.(?:dll|drv))\s*$') {
                $dependencyCount++
                $dependency = $Matches[1]
                # API-set names are Windows contracts rather than app-local DLLs.
                if (!$known.Contains($dependency) -and $dependency -notmatch '^(api|ext)-ms-' -and
                    !(Test-Path -LiteralPath (Join-Path $system $dependency) -PathType Leaf)) {
                    throw "Unresolved PE import in the native candidate: $dependency"
                }
            }
        }
        if ($dependencyCount -eq 0) { throw 'The native PE import report contained no dependencies.' }
    } finally { Pop-Location }
    Write-Output "Unqualified development candidate staged at $candidate"
} finally {
    foreach ($name in $variables) {
        [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name])
    }
}
