param(
    [switch]$DebugBuild,
    [int]$Port = 15718
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binary = Join-Path $repo "target/debug/bevy_brp_mcp.exe"
$temp = Join-Path ([IO.Path]::GetTempPath()) ("bevy-brp-diagnostics-" + [guid]::NewGuid().ToString("N"))
[void](New-Item -ItemType Directory -Path $temp)
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $binary
$start.WorkingDirectory = $repo
$start.UseShellExecute = $false
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.CreateNoWindow = $true
$start.Environment["TEMP"] = $temp
$start.Environment["TMP"] = $temp
$server = [Diagnostics.Process]::new()
$server.StartInfo = $start
$serverStarted = $false
$nextId = 0
$appLaunched = $false
$watchId = $null

function Request([string]$Method, [object]$Params, [int]$TimeoutMs = 180000) {
    $script:nextId += 1
    $request = [ordered]@{ jsonrpc = "2.0"; id = $script:nextId; method = $Method; params = $Params }
    $server.StandardInput.WriteLine(($request | ConvertTo-Json -Depth 60 -Compress))
    $server.StandardInput.Flush()
    $read = $server.StandardOutput.ReadLineAsync()
    if (-not $read.Wait($TimeoutMs)) { throw "MCP timeout: $Method" }
    if ($null -eq $read.Result) { throw "MCP server closed stdout: $Method" }
    $response = $read.Result | ConvertFrom-Json -Depth 60
    if ($response.id -ne $script:nextId) { throw "Unexpected response id for $Method" }
    return $response
}

function Tool([string]$Name, [hashtable]$Arguments, [int]$TimeoutMs = 180000) {
    $response = Request "tools/call" ([ordered]@{ name = $Name; arguments = $Arguments }) $TimeoutMs
    if ($null -ne $response.error) { throw "MCP tool error $Name`: $($response.error | ConvertTo-Json -Compress)" }
    if ($response.result.isError) { throw "Tool failed $Name`: $($response.result.structuredContent | ConvertTo-Json -Depth 30 -Compress)" }
    return $response.result.structuredContent
}

function Test-PortOpen([int]$Number) {
    $client = [Net.Sockets.TcpClient]::new()
    try {
        $connect = $client.ConnectAsync("127.0.0.1", $Number)
        return $connect.Wait(500) -and $client.Connected
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

try {
    if (Test-PortOpen $Port) { throw "BRP test port is occupied: $Port" }
    [void]$server.Start()
    $serverStarted = $true
    $init = Request "initialize" ([ordered]@{
        protocolVersion = "2025-11-25"
        capabilities = @{}
        clientInfo = [ordered]@{ name = "diagnostics-regression"; version = "1" }
    })
    if ($null -ne $init.error) { throw "MCP initialization failed" }
    $server.StandardInput.WriteLine('{"jsonrpc":"2.0","method":"notifications/initialized"}')
    $server.StandardInput.Flush()

    $listing = Request "tools/list" @{}
    if ($null -ne $listing.error) { throw "Tool listing failed" }
    $names = @($listing.result.tools | ForEach-Object name)
    $expectedCount = if ($DebugBuild) { 49 } else { 47 }
    if ($names.Count -ne $expectedCount) { throw "Expected $expectedCount tools, got $($names.Count)" }
    foreach ($name in @("brp_get_trace_log_path", "brp_set_tracing_level")) {
        if (($name -in $names) -ne [bool]$DebugBuild) { throw "Wrong visibility for $name" }
    }
    if ($DebugBuild) {
        $level = Tool "brp_set_tracing_level" @{ level = "debug" }
        $trace = Tool "brp_get_trace_log_path" @{}
        if ($level.status -ne "success" -or $trace.metadata.exists -ne $true) {
            throw "Debug trace control failed"
        }
        if (-not $trace.metadata.log_path.StartsWith($temp)) { throw "Trace outside isolated temp directory" }
    } else {
        $hidden = Request "tools/call" ([ordered]@{ name = "brp_get_trace_log_path"; arguments = @{} })
        if ($null -eq $hidden.error -or $hidden.error.message -notlike "*unknown tool*") {
            throw "Hidden trace tool remained callable"
        }
    }

    $launch = Tool "brp_launch" @{
        target_name = "test_app"
        package_name = "bevy_brp_test_apps"
        path = $repo
        port = $Port
        instance_count = 1
        search_order = "app"
    } 240000
    $appLaunched = $true
    $status = Tool "brp_status" @{ app_name = "test_app"; port = $Port }
    if ($status.status -ne "success") { throw "App did not start" }
    $query = Tool "world_query" @{ data = @{}; filter = @{ with = @("bevy_sprite::sprite::Sprite") }; port = $Port }
    $entity = [uint64]$query.result[0].entity
    $transform = "bevy_transform::components::transform::Transform"
    $watch = Tool "world_get_components_watch" @{ entity = $entity; types = @($transform); port = $Port }
    $watchId = [uint32]$watch.metadata.watch_id
    [void](Tool "world_mutate_components" @{
        entity = $entity; component = $transform; path = ".translation.x"; value = 43.0; port = $Port
    })
    Start-Sleep -Milliseconds 700

    $appLogs = Tool "brp_list_logs" @{ source = "app"; app_name = "test_app" }
    $watchLogs = Tool "brp_list_logs" @{ source = "watch" }
    if ($appLogs.metadata.log_count -lt 1 -or $watchLogs.metadata.log_count -lt 1) {
        throw "Missing application or watch log"
    }
    if (@($appLogs.result | Where-Object source -ne "app").Count -ne 0) { throw "App log filter leaked" }
    if (@($watchLogs.result | Where-Object source -ne "watch").Count -ne 0) { throw "Watch log filter leaked" }
    [void](Tool "brp_read_log" @{ filename = $appLogs.result[0].filename; tail_lines = 20 })
    [void](Tool "brp_read_log" @{ filename = $watchLogs.result[0].filename; tail_lines = 20 })
    $traceRead = Request "tools/call" ([ordered]@{
        name = "brp_read_log"; arguments = @{ filename = "bevy_brp_mcp_trace.log" }
    })
    if ($traceRead.result.isError -ne $true) { throw "Public read accepted server trace" }
    [void](Tool "brp_stop_watch" @{ watch_id = $watchId })
    $watchId = $null
    $shutdown = Tool "brp_shutdown" @{ app_name = "test_app"; port = $Port }
    if ($shutdown.metadata.shutdown_method -ne "clean_shutdown") { throw "App did not shut down cleanly" }
    $appLaunched = $false
    for ($attempt = 0; $attempt -lt 20 -and (Test-PortOpen $Port); $attempt += 1) {
        Start-Sleep -Milliseconds 250
    }
    if (Test-PortOpen $Port) { throw "BRP port remains open after shutdown: $Port" }
    [void](Tool "brp_delete_logs" @{ source = "all" })
    $after = Tool "brp_list_logs" @{}
    if ($after.metadata.log_count -ne 0) { throw "Public logs remain after cleanup" }
    if ($DebugBuild -and -not (Test-Path -LiteralPath $trace.metadata.log_path)) {
        throw "Public cleanup deleted server trace"
    }
    Write-Output "diagnostics regression passed: debug=$DebugBuild tools=$expectedCount"
} finally {
    if ($null -ne $watchId) {
        try { [void](Tool "brp_stop_watch" @{ watch_id = $watchId }) } catch { Write-Warning $_ }
    }
    if ($appLaunched) {
        try { [void](Tool "brp_shutdown" @{ app_name = "test_app"; port = $Port }) } catch { Write-Warning $_ }
    }
    if ($serverStarted -and -not $server.HasExited) { $server.Kill($true) }
    if ($serverStarted) { [void]$server.WaitForExit(5000) }
    $server.Dispose()
    $resolvedTemp = [IO.Path]::GetFullPath($temp)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $resolvedTemp.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove directory outside temp root: $resolvedTemp"
    }
    Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
}
