param(
    [int]$Port = 15712,
    [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path
$assetDir = if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    Join-Path ([IO.Path]::GetTempPath()) ("bevy-brp-baseline-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
} elseif ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path (Get-Location).Path $OutputDirectory))
}
[void](New-Item -ItemType Directory -Path $assetDir -Force)
$mcpBinary = Join-Path $repo "target\debug\bevy_brp_mcp.exe"
$screenshotPath = Join-Path $assetDir "test-app.png"
$userHome = [Environment]::GetFolderPath("UserProfile")
$utf8 = [Text.UTF8Encoding]::new($false)
$records = [Collections.Generic.List[object]]::new()
$requestCount = 0
$requestBytes = 0
$responseBytes = 0
$nextId = 1
$launchedApp = $null

if ($IsWindows) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class BaselineWindow {
    [DllImport("user32.dll")]
    public static extern bool ShowWindowAsync(IntPtr window, int command);
}
"@
}

function Protect-LocalText([string]$Text) {
    $protected = $Text
    foreach ($pathAndToken in @(
        @($script:repo, "<REPO>"),
        @($script:userHome, "<USER_HOME>")
    )) {
        $variant = $pathAndToken[0]
        $token = $pathAndToken[1]
        for ($escapeLevel = 0; $escapeLevel -lt 4; $escapeLevel += 1) {
            $protected = $protected.Replace($variant, $token)
            $variant = $variant.Replace("\", "\\")
        }
    }
    return $protected
}

function Add-WireRecord([string]$Name, [string]$Direction, [string]$Wire, [int]$Bytes) {
    $protected = Protect-LocalText $Wire
    $records.Add([ordered]@{
        name = $Name
        direction = $Direction
        bytes = $Bytes
        payload = ($protected | ConvertFrom-Json -Depth 100)
    })
}

function Send-Notification([string]$Method) {
    $message = [ordered]@{ jsonrpc = "2.0"; method = $Method }
    $wire = $message | ConvertTo-Json -Depth 100 -Compress
    $bytes = $utf8.GetByteCount($wire + $process.StandardInput.NewLine)
    $process.StandardInput.WriteLine($wire)
    $process.StandardInput.Flush()
    Add-WireRecord $Method "client_to_server" $wire $bytes
    $script:requestBytes += $bytes
}

function Send-Request([string]$Name, [string]$Method, [object]$Params, [int]$TimeoutMs = 120000) {
    $id = $script:nextId
    $script:nextId += 1
    $message = [ordered]@{ jsonrpc = "2.0"; id = $id; method = $Method; params = $Params }
    $wire = $message | ConvertTo-Json -Depth 100 -Compress
    $bytes = $utf8.GetByteCount($wire + $process.StandardInput.NewLine)
    $process.StandardInput.WriteLine($wire)
    $process.StandardInput.Flush()
    Add-WireRecord $Name "client_to_server" $wire $bytes
    $script:requestCount += 1
    $script:requestBytes += $bytes

    $read = $process.StandardOutput.ReadLineAsync()
    if (-not $read.Wait($TimeoutMs)) {
        throw "Timed out waiting for MCP response to $Name"
    }
    $responseWire = $read.Result
    if ($null -eq $responseWire) {
        throw "MCP server closed stdout while waiting for $Name"
    }
    $responseWireBytes = $utf8.GetByteCount($responseWire + $process.StandardOutput.NewLine)
    Add-WireRecord $Name "server_to_client" $responseWire $responseWireBytes
    $script:responseBytes += $responseWireBytes
    $response = $responseWire | ConvertFrom-Json -Depth 100
    if ($response.id -ne $id) {
        throw "Unexpected MCP response id for ${Name}: $($response.id), expected $id"
    }
    if ($null -ne $response.error) {
        throw "MCP protocol error for ${Name}: $($response.error | ConvertTo-Json -Depth 20 -Compress)"
    }
    return $response
}

function Call-Tool([string]$Name, [hashtable]$Arguments, [int]$TimeoutMs = 120000) {
    $response = Send-Request $Name "tools/call" ([ordered]@{
        name = $Name
        arguments = $Arguments
    }) $TimeoutMs
    if ($response.result.isError -eq $true) {
        throw "Tool ${Name} failed: $($response.result.structuredContent | ConvertTo-Json -Depth 30 -Compress)"
    }
    return $response.result.structuredContent
}

function Call-ToolExpectedError([string]$Name, [hashtable]$Arguments, [int]$TimeoutMs = 120000) {
    $response = Send-Request $Name "tools/call" ([ordered]@{
        name = $Name
        arguments = $Arguments
    }) $TimeoutMs
    if ($response.result.isError -ne $true) {
        throw "Tool $Name unexpectedly succeeded"
    }
    return $response.result.structuredContent
}

function Restore-AppWindow([string]$ProcessName) {
    if (-not $IsWindows) {
        return
    }
    for ($attempt = 0; $attempt -lt 20; $attempt += 1) {
        $appProcess = Get-Process $ProcessName -ErrorAction SilentlyContinue |
            Where-Object { $_.MainWindowHandle -ne 0 } |
            Select-Object -First 1
        if ($null -ne $appProcess) {
            [void][BaselineWindow]::ShowWindowAsync($appProcess.MainWindowHandle, 9)
            Start-Sleep -Milliseconds 500
            return
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Could not find a restorable window for $ProcessName"
}

$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $mcpBinary
$startInfo.WorkingDirectory = $repo
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardInput = $true
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.CreateNoWindow = $true
$process = [Diagnostics.Process]::new()
$process.StartInfo = $startInfo

try {
    [void]$process.Start()

    $initialize = Send-Request "initialize" "initialize" ([ordered]@{
        protocolVersion = "2025-11-25"
        capabilities = @{}
        clientInfo = [ordered]@{ name = "bevy-brp-baseline"; version = "1.0" }
    })
    Send-Notification "notifications/initialized"

    $tools = Send-Request "tools/list" "tools/list" @{}
    $toolListRecord = $records[$records.Count - 1]
    $records.RemoveAt($records.Count - 1)
    $toolListRequest = $records[$records.Count - 1]
    $records.RemoveAt($records.Count - 1)
    [IO.File]::WriteAllText(
        (Join-Path $assetDir "tools-list.json"),
        (([ordered]@{
            request = $toolListRequest
            response = $toolListRecord
        } | ConvertTo-Json -Depth 100).Replace("`r`n", "`n")),
        $utf8
    )

    $traceLevel = Call-Tool "brp_set_tracing_level" @{ level = "debug" }
    $tracePath = Call-Tool "brp_get_trace_log_path" @{}
    $targets = Call-Tool "brp_list_bevy" @{ path = $repo }
    $launch = Call-Tool "brp_launch" @{
        target_name = "test_app"
        package_name = "bevy_brp_test_apps"
        path = $repo
        port = $Port
        instance_count = 1
        search_order = "app"
        args = @("--marker", "baseline-01")
    } 240000
    $launchedApp = "test_app"
    $status = Call-Tool "brp_status" @{ app_name = "test_app"; port = $Port }
    $discovery = Call-Tool "rpc_discover" @{ port = $Port }

    $spriteQuery = Call-Tool "world_query" @{
        data = @{}
        filter = @{ with = @("bevy_sprite::sprite::Sprite") }
        port = $Port
    }
    $spriteEntity = [uint64]$spriteQuery.result[0].entity
    $transformType = "bevy_transform::components::transform::Transform"
    $before = Call-Tool "world_get_components" @{
        entity = $spriteEntity
        components = @($transformType)
        port = $Port
    }
    $watch = Call-Tool "world_get_components_watch" @{
        entity = $spriteEntity
        types = @($transformType)
        port = $Port
    }
    $mutate = Call-Tool "world_mutate_components" @{
        entity = $spriteEntity
        component = $transformType
        path = ".translation.x"
        value = 42.0
        port = $Port
    }
    $after = Call-Tool "world_get_components" @{
        entity = $spriteEntity
        components = @($transformType)
        port = $Port
    }
    Start-Sleep -Milliseconds 500
    $activeWatches = Call-Tool "brp_list_active_watches" @{}
    $stopWatch = Call-Tool "brp_stop_watch" @{ watch_id = [int]$watch.metadata.watch_id }

    $runtimeInput = Call-Tool "brp_extras_type_text" @{ text = "baseline"; port = $Port }
    $runtimePrimaryScreenshot = Call-ToolExpectedError "brp_extras_screenshot" @{
        path = (Join-Path $assetDir "test-app-primary.png")
        port = $Port
    } 60000
    $runtimeLogs = Call-Tool "brp_list_logs" @{ app_name = "test_app"; verbose = $true }
    $runtimeLogFilename = [string]$runtimeLogs.result[0].filename
    $runtimeLog = Call-Tool "brp_read_log" @{ filename = $runtimeLogFilename; tail_lines = 40 }
    $runtimeShutdown = Call-Tool "brp_shutdown" @{ app_name = "test_app"; port = $Port }
    $launchedApp = $null

    $fixtureLaunch = Call-Tool "brp_launch" @{
        target_name = "extras_plugin"
        package_name = "bevy_brp_test_apps"
        path = $repo
        port = $Port
        instance_count = 1
        search_order = "example"
    } 240000
    $launchedApp = "extras_plugin"
    $fixtureStatus = Call-Tool "brp_status" @{ app_name = "extras_plugin"; port = $Port }
    Restore-AppWindow "extras_plugin"
    $fixtureEntity = Call-Tool "world_find_entities_by_name" @{
        name = "NatesList"
        match_mode = "exact"
        port = $Port
    }
    Start-Sleep -Seconds 2
    $screenshotWarmup = Call-Tool "brp_extras_screenshot" @{
        name = "NatesList"
        path = $screenshotPath
        port = $Port
    } 60000
    Start-Sleep -Seconds 1
    $screenshot = Call-Tool "brp_extras_screenshot" @{
        name = "NatesList"
        path = $screenshotPath
        port = $Port
    } 60000
    $fixtureInput = Call-Tool "brp_extras_type_text" @{ text = "baseline"; port = $Port }
    $fixtureLogs = Call-Tool "brp_list_logs" @{ app_name = "extras_plugin"; verbose = $true }
    $fixtureLogFilename = [string]$fixtureLogs.result[0].filename
    $fixtureLog = Call-Tool "brp_read_log" @{ filename = $fixtureLogFilename; tail_lines = 40 }
    $fixtureShutdown = Call-Tool "brp_shutdown" @{ app_name = "extras_plugin"; port = $Port }
    $launchedApp = $null

    $transcriptLines = foreach ($record in $records) {
        $record | ConvertTo-Json -Depth 100 -Compress
    }
    [IO.File]::WriteAllText(
        (Join-Path $assetDir "interaction.jsonl"),
        (($transcriptLines -join "`n") + "`n"),
        $utf8
    )

    $summary = [ordered]@{
        protocol_requested = "2025-11-25"
        protocol_negotiated = $initialize.result.protocolVersion
        tool_count = $tools.result.tools.Count
        tool_names = @($tools.result.tools.name)
        mcp_request_count = $requestCount
        tool_call_count = $requestCount - 2
        notification_count = 1
        client_to_server_bytes = $requestBytes
        server_to_client_bytes = $responseBytes
        port = $Port
        runtime_target = "bevy_brp_test_apps/test_app"
        screenshot_target = "bevy_brp_test_apps/examples/extras_plugin"
        entity = $spriteEntity
        rpc_method_count = $discovery.metadata.method_count
        watch_id = $watch.metadata.watch_id
        watch_log = Split-Path -Leaf $watch.metadata.log_path
        runtime_input_chars_queued = $runtimeInput.metadata.chars_queued
        runtime_primary_screenshot_status = $runtimePrimaryScreenshot.status
        runtime_primary_screenshot_message = $runtimePrimaryScreenshot.message
        fixture_input_chars_queued = $fixtureInput.metadata.chars_queued
        screenshot_file = Split-Path -Leaf $screenshotPath
        screenshot_bytes = (Get-Item $screenshotPath).Length
        runtime_app_log = $runtimeLogFilename
        fixture_app_log = $fixtureLogFilename
        runtime_shutdown_method = $runtimeShutdown.metadata.shutdown_method
        fixture_shutdown_method = $fixtureShutdown.metadata.shutdown_method
        model_rounds = $null
        client_token_usage = $null
        client_cache_input = $null
    }
    [IO.File]::WriteAllText(
        (Join-Path $assetDir "summary.json"),
        (($summary | ConvertTo-Json -Depth 20).Replace("`r`n", "`n")),
        $utf8
    )
    $summary | ConvertTo-Json -Depth 20
}
finally {
    if ($null -ne $launchedApp) {
        try {
            [void](Call-Tool "brp_shutdown" @{ app_name = $launchedApp; port = $Port })
        }
        catch {
            Get-Process "test_app" -ErrorAction SilentlyContinue | Stop-Process
        }
    }
    if ($null -ne $process -and -not $process.HasExited) {
        $process.StandardInput.Close()
        if (-not $process.WaitForExit(5000)) {
            $process.Kill()
        }
    }
}
