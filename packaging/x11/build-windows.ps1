param(
    [Parameter(Mandatory=$true)][string]$CandidateDirectory,
    [string]$Archive,
    [string]$SevenZip = "7z.exe"
)
$ErrorActionPreference = "Stop"
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "Stage the VcXsrv candidate on Windows."
}
$CandidateDirectory = [System.IO.Path]::GetFullPath($CandidateDirectory)
if (Test-Path -LiteralPath $CandidateDirectory) {
    throw "Candidate destination already exists."
}
$url = "https://github.com/marchaesen/vcxsrv/releases/download/21.1.16.1/vcxsrv-64.21.1.16.1.installer.noadmin.exe"
$sha = "DEA6C7D67D3D15B4ED45C87B63A83C88F4ACEAAEF5425630F0E97A0BAD70D620"
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("termcraft-vcxsrv-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $pinned = Join-Path $temporary "upstream.exe"
    if ($Archive) {
        Copy-Item -LiteralPath $Archive -Destination $pinned
    } else {
        Invoke-WebRequest -Uri $url -OutFile $pinned
    }
    if ((Get-FileHash -LiteralPath $pinned -Algorithm SHA256).Hash -ne $sha) {
        throw "VcXsrv archive checksum mismatch."
    }
    $payload = Join-Path $temporary "candidate"
    # Read NSIS payload files as an archive. The installer is never executed.
    & $SevenZip x "-o$payload" $pinned
    if ($LASTEXITCODE -ne 0) { throw "7-Zip could not extract the pinned VcXsrv payload." }
    foreach ($inputFile in @("vcxsrv.exe", "xkbcomp.exe", "xkbdata")) {
        if (-not (Test-Path -LiteralPath (Join-Path $payload $inputFile))) {
            throw "Missing expected VcXsrv input $inputFile."
        }
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "manifest.json") -Destination $payload
    Move-Item -LiteralPath $payload -Destination $CandidateDirectory
    Write-Output "Staged unqualified VcXsrv candidate for native review at $CandidateDirectory"
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
