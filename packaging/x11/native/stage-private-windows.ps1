[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$CandidateDirectory,
    [Parameter(Mandatory)][string]$CrtDirectory
)

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $SourceDirectory).Path
$crt = (Resolve-Path -LiteralPath $CrtDirectory).Path
$candidate = [IO.Path]::GetFullPath($CandidateDirectory)
if (Test-Path -LiteralPath $candidate) { throw 'Candidate directory already exists.' }

$paths = @(
    'xorg-server/obj64/servrelease/vcxsrv.exe'
    'xkbcomp/obj64/release/xkbcomp.exe'
    'libX11/obj64/release/libX11.dll'
    'libxcb/src/obj64/release/libxcb.dll'
    'libXau/obj64/release/libXau.dll'
    'libXext/src/obj64/release/libXext.dll'
    'libXmu/src/obj64/release/libXmu.dll'
    'zlib/obj64/release/zlib1.dll'
    'freetype/objs/x64/Release/freetype.dll'
    'openssl/release64/libcrypto-3-x64.dll'
    'mesalib/src/obj64/release/swrast_dri.dll'
    'xorg-server/hw/xwin/swrastwgl_dri/obj64/release/swrastwgl_dri.dll'
    'dxtn/obj64/release/dxtn.dll'
    'libxml2/bin64/libxml2-2.dll'
    'libxml2/bin64/libgcc_s_sjlj-1.dll'
    'libxml2/bin64/libiconv-2.dll'
    'libxml2/bin64/libwinpthread-1.dll'
    'xorg-server/system.XWinrc'
    'xorg-server/font-dirs'
    'xorg-server/dix/protocol.txt'
    'xorg-server/XKeysymDB'
    'libX11/src/XErrorDB'
    'xorg-server/fonts'
    'xorg-server/xkbdata'
    'xorg-server/locale'
    'xorg-server/bitmaps'
)
$inputs = @($paths | ForEach-Object { Join-Path $source $_ })
$inputs += @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll' |
    ForEach-Object { Join-Path $crt $_ })
foreach ($inputPath in $inputs) {
    if (!(Test-Path -LiteralPath $inputPath)) { throw "Missing native build output: $inputPath" }
}
foreach ($path in @('fonts/misc/fonts.dir', 'xkbdata/rules/xorg', 'xkbdata/symbols/us')) {
    if (!(Test-Path -LiteralPath (Join-Path $source "xorg-server/$path") -PathType Leaf)) {
        throw "Missing generated native resource: $path"
    }
}

$null = New-Item -ItemType Directory -Path $candidate
foreach ($inputPath in $inputs) {
    Copy-Item -LiteralPath $inputPath -Destination $candidate -Recurse -Force
}
# This report is for review. Application trust comes only from compiled pins.
$hashes = [ordered]@{}
foreach ($file in Get-ChildItem -LiteralPath $candidate -File -Recurse -Force | Sort-Object FullName) {
    $relative = $file.FullName.Substring($candidate.Length + 1).Replace('\', '/')
    $hashes[$relative] = (Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant()
}
[ordered]@{
    schema = 1
    status = 'unqualified native build'
    source_contract_sha256 = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'source-contract.json')).Hash.ToLowerInvariant()
    files = $hashes
} | ConvertTo-Json -Depth 4
