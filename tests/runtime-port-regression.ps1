$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$binary = Join-Path $repo 'target/debug/examples/runtime_custom_port.exe'
$mainPort = 15752
$defaultPort = 15702
$renderPort = 15703

if (-not $IsWindows) { throw 'The runtime port example requires Windows.' }
if (-not (Test-Path -LiteralPath $binary)) { throw "Build the runtime custom-port example first: $binary" }
foreach ($port in @($mainPort, $defaultPort, $renderPort)) {
    if (Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue) {
        throw "Port $port is already in use"
    }
}

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $binary
$start.WorkingDirectory = $repo
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
[void]$start.Environment.Remove('BRP_EXTRAS_PORT')
$app = [Diagnostics.Process]::Start($start)
try {
    $listening = $false
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        if (Get-NetTCPConnection -State Listen -LocalPort $mainPort -ErrorAction SilentlyContinue) {
            $listening = $true
            break
        }
        if ($app.HasExited) { throw "Runtime example exited before listening: $($app.ExitCode)" }
        Start-Sleep -Milliseconds 250
    }
    if (-not $listening) { throw "Runtime example did not listen on port $mainPort" }
    if (Get-NetTCPConnection -State Listen -LocalPort $defaultPort -ErrorAction SilentlyContinue) {
        throw "Runtime example unexpectedly listened on default port $defaultPort"
    }

    $discover = Invoke-RestMethod -Uri "http://127.0.0.1:$mainPort" -Method Post `
        -ContentType 'application/json' `
        -Body '{"jsonrpc":"2.0","id":1,"method":"rpc.discover"}' -TimeoutSec 15
    if (@($discover.result.methods).Count -lt 1) { throw 'BRP discovery returned no methods' }

    $shutdown = Invoke-RestMethod -Uri "http://127.0.0.1:$mainPort" -Method Post `
        -ContentType 'application/json' `
        -Body '{"jsonrpc":"2.0","id":2,"method":"brp_extras/shutdown"}' -TimeoutSec 15
    if ($null -ne $shutdown.error) { throw "BRP shutdown failed: $($shutdown.error.message)" }
    if (-not $app.WaitForExit(15000)) { throw 'Runtime example did not shut down' }
    if (Get-NetTCPConnection -State Listen -LocalPort $mainPort -ErrorAction SilentlyContinue) {
        throw "Port $mainPort remains open after shutdown"
    }
    Write-Output "Runtime custom port regression passed: Main=$mainPort, methods=$(@($discover.result.methods).Count), exit=$($app.ExitCode)"
} finally {
    if (-not $app.HasExited) {
        $app.Kill()
        [void]$app.WaitForExit(5000)
    }
    $app.Dispose()
}
