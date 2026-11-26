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
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$createdPort = $createdQueue = $createdDriver = $false
$spoolerWasRunning = (Get-Service Spooler).Status -eq 'Running'
try {
    $listener.Start()
    if (!$spoolerWasRunning) { Start-Service Spooler }
    Add-PrinterPort -Name $queue -PrinterHostAddress 127.0.0.1 -PortNumber $listener.LocalEndpoint.Port
    $createdPort = $true
    $rawPort = Get-PrinterPort -Name $queue
    if ($rawPort.PrinterHostAddress -ne '127.0.0.1' -or $rawPort.PortNumber -ne $listener.LocalEndpoint.Port -or $rawPort.SNMPEnabled) {
        throw 'The disposable RAW port did not retain its loopback settings.'
    }
    if (!(Get-PrinterDriver -Name $driver -ErrorAction SilentlyContinue)) {
        Add-PrinterDriver -Name $driver
        $createdDriver = $true
    }
    Add-Printer -Name $queue -DriverName $driver -PortName $queue
    $createdQueue = $true

    $payload = [Text.Encoding]::ASCII.GetBytes("TermCraft Winspool fixture`r`n")
    $initialize = [byte[]](@(1) + [Text.Encoding]::UTF8.GetBytes($queue))
    $begin = [byte[]](@(3) + [BitConverter]::GetBytes([uint32]$payload.Length))
    $write = [byte[]](@(4) + $payload)
    Invoke-PrinterWorker @($initialize, [byte[]]@(2), $begin, [byte[]]@(6))
    if ($listener.Pending() -or (Get-PrintJob -PrinterName $queue)) {
        throw 'The aborted printer worker left a job or connected to the RAW port.'
    }
    Invoke-PrinterWorker @($initialize, $begin, $write, [byte[]]@(5))
    $received = $listener.AcceptTcpClientAsync().WaitAsync([TimeSpan]::FromSeconds(15)).GetAwaiter().GetResult()
    try {
        $stream = $received.GetStream()
        $stream.ReadTimeout = 5000
        $actual = [byte[]]::new($payload.Length)
        $stream.ReadExactly($actual)
        if (![Linq.Enumerable]::SequenceEqual[byte]($actual, $payload) -or $stream.ReadByte() -ne -1) {
            throw 'The RAW spool monitor sent different bytes.'
        }
    } finally {
        $received.Dispose()
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while (Get-PrintJob -PrinterName $queue) {
        if ($timer.Elapsed.TotalSeconds -gt 10) { throw 'The completed job remains queued.' }
        Start-Sleep -Milliseconds 100
    }
    Get-FileHash -LiteralPath $helperPath
    Write-Output 'Winspool accepted the exact RAW bytes and the aborted job left no output.'
} catch {
    Write-Warning $_.Exception.Message
    throw
} finally {
    $listener.Stop()
    if ($createdQueue) { Remove-Printer -Name $queue -ErrorAction Continue }
    if ($createdPort) { Remove-PrinterPort -Name $queue -ErrorAction Continue }
    if ($createdDriver) { Remove-PrinterDriver -Name $driver -ErrorAction Continue }
    if (Get-Printer -Name $queue -ErrorAction SilentlyContinue) { throw 'Fixture printer queue remains.' }
    if (Get-PrinterPort -Name $queue -ErrorAction SilentlyContinue) { throw 'Fixture printer port remains.' }
    if (!$spoolerWasRunning -and (Get-Service Spooler).Status -eq 'Running') { Stop-Service Spooler }
}
