$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$binary = Join-Path $repo 'target/debug/examples/runtime_custom_port.exe'
$mainPort = 15752
$renderPort = 15753
$mainOverride = 15754
$renderOverride = 15755
$testPorts = @($mainPort, $renderPort, $mainOverride, $renderOverride)
$observedPorts = @(15702, 15703) + $testPorts

if (-not $IsWindows) { throw 'The runtime port example requires Windows.' }
if (-not (Test-Path -LiteralPath $binary)) { throw "Build the runtime custom-port example first: $binary" }

function Assert-PortsFree {
    $listeners = @(Get-NetTCPConnection -State Listen -LocalPort $testPorts -ErrorAction SilentlyContinue)
    if ($listeners.Count -ne 0) { throw "Ports still in use: $($listeners.LocalPort -join ', ')" }
}

function Start-Fixture([hashtable]$Overrides) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.WorkingDirectory = $repo
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    [void]$start.Environment.Remove('BRP_EXTRAS_PORT')
    [void]$start.Environment.Remove('BRP_EXTRAS_RENDER_PORT')
    $start.Environment['BRP_EXTRAS_WINDOW_FOCUSED'] = 'false'
    foreach ($entry in $Overrides.GetEnumerator()) {
        $start.Environment[$entry.Key] = [string]$entry.Value
    }
    [Diagnostics.Process]::Start($start)
}

function Invoke-Brp([int]$Port, [string]$Method) {
    $body = @{ jsonrpc = '2.0'; id = 1; method = $Method } | ConvertTo-Json -Compress
    $response = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post `
        -ContentType 'application/json' -Body $body -TimeoutSec 15
    if ($null -ne $response.error) { throw "$Method failed on ${Port}: $($response.error.message)" }
    $response.result
}

function Stop-Fixture([Diagnostics.Process]$App) {
    try {
        if (-not $App.HasExited) {
            $App.Kill()
            if (-not $App.WaitForExit(5000)) { throw 'Runtime example did not exit after cleanup' }
        }
    } finally {
        $App.Dispose()
    }
}

$cases = @(
    @{ Name = 'code configuration'; Main = $mainPort; Render = $renderPort; Overrides = @{} }
    @{ Name = 'both environment overrides'; Main = $mainOverride; Render = $renderOverride;
       Overrides = @{ BRP_EXTRAS_PORT = $mainOverride; BRP_EXTRAS_RENDER_PORT = $renderOverride } }
    @{ Name = 'invalid Render override'; Main = $mainOverride; Render = $renderPort;
       Overrides = @{ BRP_EXTRAS_PORT = $mainOverride; BRP_EXTRAS_RENDER_PORT = 'invalid' } }
    @{ Name = 'invalid Main override'; Main = $mainPort; Render = $renderOverride;
       Overrides = @{ BRP_EXTRAS_PORT = 'invalid'; BRP_EXTRAS_RENDER_PORT = $renderOverride } }
)

Assert-PortsFree
foreach ($case in $cases) {
    $app = Start-Fixture $case.Overrides
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            if ($app.HasExited) { throw "Runtime example exited before listening: $($app.ExitCode)" }
            $listeners = @(Get-NetTCPConnection -State Listen -OwningProcess $app.Id `
                -LocalPort $observedPorts -ErrorAction SilentlyContinue)
            if ($case.Main -in $listeners.LocalPort -and $case.Render -in $listeners.LocalPort) { break }
            if ([DateTime]::UtcNow -ge $deadline) { throw "Listeners did not start for $($case.Name)" }
            Start-Sleep -Milliseconds 100
        } while ($true)

        if (@($listeners.LocalPort | Where-Object { $_ -notin @($case.Main, $case.Render) }).Count) {
            throw "Runtime example opened an unexpected port for $($case.Name)"
        }
        foreach ($port in @($case.Main, $case.Render)) {
            $discovery = Invoke-Brp $port 'rpc.discover'
            if (-not ($discovery.methods | Where-Object { $_.name } | Select-Object -First 1)) {
                throw "BRP discovery returned no methods on $port"
            }
        }
        $null = Invoke-Brp $case.Main 'brp_extras/shutdown'
        if (-not $app.WaitForExit(15000) -or $app.ExitCode -ne 0) { throw 'Runtime example did not shut down cleanly' }
        Write-Output "Passed $($case.Name) with Main=$($case.Main) and Render=$($case.Render)"
    } finally {
        Stop-Fixture $app
    }
    Assert-PortsFree
}

foreach ($occupiedPort in @($mainPort, $renderPort)) {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $occupiedPort)
    $listener.ExclusiveAddressUse = $true
    $listener.Start()
    try {
        $app = Start-Fixture @{}
        try {
            if (-not $app.WaitForExit(15000) -or $app.ExitCode -eq 0) {
                throw "Runtime example did not report bind failure on $occupiedPort"
            }
            Write-Output "Passed shared shutdown after bind failure on $occupiedPort"
        } finally {
            Stop-Fixture $app
        }
    } finally {
        $listener.Stop()
    }
    Assert-PortsFree
}
