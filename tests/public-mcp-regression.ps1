param([int]$Port = 15742)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$binary = Join-Path $repo 'target/debug/bevy_brp_mcp.exe'
if (-not (Test-Path -LiteralPath $binary)) { throw "Build the MCP binary first: $binary" }
if (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) {
    throw "Port $Port is already in use"
}
if (Get-Process extras_plugin -ErrorAction SilentlyContinue) {
    throw 'An extras_plugin process is already running'
}

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $binary
$start.WorkingDirectory = $repo
$start.UseShellExecute = $false
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.CreateNoWindow = $true
$server = [Diagnostics.Process]::new()
$server.StartInfo = $start
$id = 0
$serverStarted = $false
$launched = $false
$launchedPid = $null

function Request([string]$Method, [object]$Params, [int]$TimeoutMs = 120000) {
    $script:id++
    $wire = @{ jsonrpc = '2.0'; id = $script:id; method = $Method; params = $Params } |
        ConvertTo-Json -Depth 100 -Compress
    $server.StandardInput.WriteLine($wire)
    $server.StandardInput.Flush()
    $read = $server.StandardOutput.ReadLineAsync()
    if (-not $read.Wait($TimeoutMs)) { throw "MCP $Method timed out" }
    if ($null -eq $read.Result) { throw "MCP $Method closed stdout" }
    $response = $read.Result | ConvertFrom-Json -Depth 100
    if ($response.id -ne $script:id -or $null -ne $response.error) {
        throw "MCP $Method protocol error: $($read.Result)"
    }
    return $response.result
}

function Tool([string]$Name, [hashtable]$Arguments, [bool]$ErrorExpected = $false, [int]$TimeoutMs = 120000) {
    $response = Request 'tools/call' @{ name = $Name; arguments = $Arguments } $TimeoutMs
    if ([bool]$response.isError -ne $ErrorExpected) {
        throw "Unexpected $Name outcome: $($response.structuredContent | ConvertTo-Json -Depth 30 -Compress)"
    }
    return $response.structuredContent
}

function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

try {
    [void]$server.Start()
    $serverStarted = $true
    [void](Request 'initialize' @{
        protocolVersion = '2025-11-25'
        capabilities = @{}
        clientInfo = @{ name = 'public-mcp-regression'; version = '1.0' }
    })
    $server.StandardInput.WriteLine('{"jsonrpc":"2.0","method":"notifications/initialized"}')
    $server.StandardInput.Flush()

    $listed = Request 'tools/list' @{}
    Assert ($listed.tools.Count -ge 47) 'Expected the public native tool catalog'
    Assert ('brp_execute' -in @($listed.tools.name)) 'brp_execute is absent'
    $badPort = Tool 'rpc_discover' @{ port = '15742' } $true
    Assert ($badPort.error_info.stage -eq 'parameter_validation') 'Invalid port did not return structured parameter error'
    $launch = Tool 'brp_launch' @{
        target_name = 'extras_plugin'
        package_name = 'bevy_brp_test_apps'
        search_order = 'example'
        path = 'tests/test-app'
        port = $Port
    } $false 300000
    $launched = $true
    $launchedPid = [int]$launch.result[0].pid

    $listening = $false
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        if (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) {
            $listening = $true
            break
        }
        if (-not (Get-Process -Id $launchedPid -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 250
    }
    Assert $listening "Launched extras_plugin did not listen on port $Port"

    $catalog = Tool 'brp_list_agent_tools' @{ port = $Port }
    $multiply = @($catalog.result.tools | Where-Object name -eq 'test_multiply')
    Assert ($multiply.Count -eq 1 -and $multiply[0].method -eq 'test/multiply') 'Live app method is missing from the catalog'
    Assert ($null -ne $multiply[0].params_schema -and $null -ne $multiply[0].result_schema) 'Raw app schemas are missing'

    $product = Tool 'brp_execute' @{ method = $multiply[0].method; params = @{ value = 6; factor = 7 }; port = $Port }
    Assert ($product.result.product -eq 42) 'Dynamic method returned the wrong product'

    $invalid = Tool 'brp_execute' @{ method = 'test/multiply'; params = @{ value = 'bad'; factor = 7 }; port = $Port } $true
    Assert ($invalid.error_info.method -eq 'test/multiply' -and $invalid.error_info.port -eq $Port) 'BRP error lost method or port'
    Assert ($null -ne $invalid.error_info.code -and $invalid.metadata.code -eq $invalid.error_info.code) 'BRP error lost code or compatibility metadata'

    $typed = Tool 'world_get_resources' @{ resource = 'test::MissingResource'; port = $Port } $true
    Assert ($typed.error_info.method -eq 'world.get_resources' -and $typed.error_info.port -eq $Port) 'Typed BRP error lost method or port'
    Assert ($null -ne $typed.error_info.code) 'Typed BRP error lost its code'

    $missing = Tool 'brp_execute' @{ method = 'test/missing'; port = $Port } $true
    Assert ($missing.error_info.stage -eq 'discovery' -and 'test/multiply' -in @($missing.error_info.available_methods)) 'Missing method did not return discovery guidance'

    $watch = Tool 'brp_execute' @{ method = 'world.list_components+watch'; params = @{ entity = 0 }; port = $Port } $true
    Assert ($watch.error_info.stage -eq 'unsupported_call_mode' -and $watch.error_info.method -eq 'world.list_components+watch') 'Watching method was treated as an instant call'

    Write-Output 'Public MCP catalog, dynamic execution, BRP errors, and watch rejection passed.'
} finally {
    if ($launched) {
        try { [void](Tool 'brp_shutdown' @{ app_name = 'extras_plugin'; port = $Port }) }
        catch { Write-Warning "Clean shutdown failed: $($_.Exception.Message)" }
    }
    if ($serverStarted -and -not $server.HasExited) {
        $server.StandardInput.Close()
        if (-not $server.WaitForExit(5000)) { $server.Kill(); [void]$server.WaitForExit(5000) }
    }
    $server.Dispose()
    $cleanupPids = @()
    if ($null -ne $launchedPid) { $cleanupPids += $launchedPid }
    $cleanupPids += @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess)
    foreach ($candidatePid in @($cleanupPids | Select-Object -Unique)) {
        $candidate = Get-Process -Id $candidatePid -ErrorAction SilentlyContinue
        if ($null -ne $candidate -and $candidate.ProcessName -eq 'extras_plugin') {
            Stop-Process -Id $candidatePid -Force
        }
    }
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        if (-not (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) -and
            -not (Get-Process extras_plugin -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 250
    }
    if (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) {
        throw "Port $Port remained in use"
    }
    if (Get-Process extras_plugin -ErrorAction SilentlyContinue) {
        throw 'extras_plugin remained running'
    }
}
