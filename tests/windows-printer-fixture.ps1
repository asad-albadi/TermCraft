param([Parameter(Mandatory)][string]$Helper)

$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Run this print queue fixture on a disposable GitHub-hosted Windows runner.'
}

function Invoke-PrinterWorker($Frames) {
    $info = [Diagnostics.ProcessStartInfo]::new($helperPath)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.ArgumentList.Add('--printer-worker')
    $child = [Diagnostics.Process]::Start($info)
    try {
        foreach ($frame in $Frames) {
            $inputStream = $child.StandardInput.BaseStream
            $inputStream.Write([BitConverter]::GetBytes([uint32]$frame.Length))
            $inputStream.Write($frame)
            $inputStream.Flush()
            $reply = [byte[]]::new(8)
            $read = $child.StandardOutput.BaseStream.ReadExactlyAsync([Memory[byte]]::new($reply)).AsTask()
            if (!$read.Wait(10000)) { throw "Printer worker timed out at operation $($frame[0])." }
            $size = [BitConverter]::ToUInt32($reply, 0)
            $status = [BitConverter]::ToUInt32($reply, 4)
            if ($size -ne 4 -or $status -ne 0) {
                throw ('Printer worker operation {0} returned size {1} and status 0x{2:x8}.' -f $frame[0], $size, $status)
            }
        }
        $child.StandardInput.Close()
        if (!$child.WaitForExit(10000) -or $child.ExitCode -ne 0) {
            throw 'The printer worker did not exit successfully after the last call.'
        }
    } finally {
        if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }
        $child.Dispose()
    }
}

$helperPath = (Resolve-Path -LiteralPath $Helper).Path
$driver = 'Generic / Text Only'
$queue = 'TermCraft-Print-' + [guid]::NewGuid().ToString('N')
$port = Join-Path $env:RUNNER_TEMP "$queue.prn"
$createdPort = $createdQueue = $createdDriver = $false
$spoolerWasRunning = (Get-Service Spooler).Status -eq 'Running'
try {
    if (!$spoolerWasRunning) { Start-Service Spooler }
    if (!(Get-PrinterDriver -Name $driver -ErrorAction SilentlyContinue)) {
        Add-PrinterDriver -Name $driver
        $createdDriver = $true
    }
    Add-PrinterPort -Name $port
    $createdPort = $true
    Add-Printer -Name $queue -DriverName $driver -PortName $port
    $createdQueue = $true

    $payload = [Text.Encoding]::ASCII.GetBytes("TermCraft Winspool fixture`r`n")
    $initialize = [byte[]](@(1) + [Text.Encoding]::UTF8.GetBytes($queue))
    $begin = [byte[]](@(3) + [BitConverter]::GetBytes([uint32]$payload.Length))
    $write = [byte[]](@(4) + $payload)
    Invoke-PrinterWorker @($initialize, [byte[]]@(2), $begin, [byte[]]@(6))
    if ((Test-Path -LiteralPath $port) -or @(Get-PrintJob -PrinterName $queue).Count) {
        throw 'The aborted printer worker left a job or created the port output.'
    }
    Invoke-PrinterWorker @($initialize, $begin, $write, [byte[]]@(5))
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        if ((Test-Path -LiteralPath $port) -and !(Get-PrintJob -PrinterName $queue)) { break }
        Start-Sleep -Milliseconds 100
    } while ($timer.Elapsed.TotalSeconds -lt 15)
    $actual = [IO.File]::ReadAllBytes($port)
    if (![Linq.Enumerable]::SequenceEqual[byte]($actual, $payload) -or (Get-PrintJob -PrinterName $queue)) {
        throw 'The completed Winspool job has different bytes or remains queued.'
    }
    Get-FileHash -LiteralPath $helperPath, $port
    Write-Output 'Winspool accepted the exact RAW bytes and the aborted job left no output.'
} finally {
    if ($createdQueue) { Remove-Printer -Name $queue }
    if ($createdPort) { Remove-PrinterPort -Name $port }
    if ($createdDriver) { Remove-PrinterDriver -Name $driver }
    if (!$spoolerWasRunning -and (Get-Service Spooler).Status -eq 'Running') { Stop-Service Spooler }
}
