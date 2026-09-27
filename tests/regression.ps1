param(
    [int]$Port = 15712,
    [string]$OutputDirectory,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$baseline = Join-Path $repo 'plans/refactor/assets/01-baseline/replay.ps1'
$outputWasRequested = -not [string]::IsNullOrWhiteSpace($OutputDirectory)
$artifacts = if ($outputWasRequested) {
    [IO.Path]::GetFullPath($OutputDirectory)
} else {
    Join-Path ([IO.Path]::GetTempPath()) ('bevy-brp-regression-' + [guid]::NewGuid())
}
$traceLog = Join-Path ([IO.Path]::GetTempPath()) 'bevy_brp_mcp_trace.log'
$traceLogExisted = Test-Path -LiteralPath $traceLog
$traceLogLength = if ($traceLogExisted) { (Get-Item -LiteralPath $traceLog).Length } else { 0 }

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Responses([string]$Name) {
    @($script:records | Where-Object {
        $_.direction -eq 'server_to_client' -and $_.name -eq $Name
    } | ForEach-Object { $_.payload.result.structuredContent })
}

function Write-FailureSummary([string]$Message) {
    try {
        $lastRecord = if ($null -ne $script:records -and $script:records.Count -gt 0) {
            $script:records[$script:records.Count - 1]
        } else {
            $null
        }
        [IO.File]::WriteAllText(
            (Join-Path $script:artifacts 'failure.json'),
            ([ordered]@{
                message = $Message
                last_record_name = $lastRecord.name
                last_record_direction = $lastRecord.direction
            } | ConvertTo-Json -Depth 10),
            [Text.UTF8Encoding]::new($false)
        )
        return $null
    } catch {
        return "Failed to write failure.json: $($_.Exception.Message)"
    }
}

function Write-FailureArtifacts([string]$Message) {
    $diagnosticFailures = @()
    $summaryFailure = Write-FailureSummary $Message
    if ($null -ne $summaryFailure) {
        $diagnosticFailures += $summaryFailure
    }
    try {
        if ($null -ne $script:records -and $script:records.Count -gt 0) {
            $transcriptLines = foreach ($record in $script:records) {
                $record | ConvertTo-Json -Depth 100 -Compress
            }
            [IO.File]::WriteAllText(
                (Join-Path $script:artifacts 'interaction.jsonl'),
                (($transcriptLines -join "`n") + "`n"),
                [Text.UTF8Encoding]::new($false)
            )
        }
    } catch {
        $diagnosticFailures += "Failed to write the partial interaction transcript: $($_.Exception.Message)"
    }
    return $diagnosticFailures
}

Assert-True ($IsWindows) 'The screenshot fixture requires an interactive Windows desktop.'
Assert-True ($Port -ge 1 -and $Port -le 65535) 'Port must be between 1 and 65535.'
Assert-True (-not (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)) "Port $Port is already in use."
Assert-True (-not (Get-Process test_app, extras_plugin -ErrorAction SilentlyContinue)) 'A test app is already running; use an isolated session.'
Assert-True (-not (Get-Process bevy_brp_mcp -ErrorAction SilentlyContinue)) 'An MCP server is already running; trace cleanup requires an isolated session.'
Assert-True (-not (Test-Path -LiteralPath $artifacts)) "Output directory already exists: $artifacts"

$passed = $false
$failure = $null
$process = $null
$processStarted = $false
$cleanupFailures = @()
try {
    . $baseline -Port $Port -OutputDirectory $artifacts | Out-Null

    $summary = Get-Content -LiteralPath (Join-Path $artifacts 'summary.json') -Raw | ConvertFrom-Json -Depth 100
    $script:records = @(Get-Content -LiteralPath (Join-Path $artifacts 'interaction.jsonl') | ForEach-Object { $_ | ConvertFrom-Json -Depth 100 })
    $catalog = Get-Content -LiteralPath (Join-Path $artifacts 'tools-list.json') -Raw | ConvertFrom-Json -Depth 100
    $names = @($catalog.response.payload.result.tools.name)
    foreach ($name in @('brp_launch', 'rpc_discover', 'world_query', 'world_get_components', 'world_mutate_components', 'world_get_components_watch', 'brp_stop_watch', 'brp_extras_screenshot', 'brp_shutdown')) {
        Assert-True ($name -in $names) "MCP tools/list omitted $name"
    }
    Assert-True ($catalog.response.payload.result.cacheScope -eq 'private') 'MCP tool list cache scope changed.'
    Assert-True ($catalog.response.payload.result.ttlMs -eq 0) 'MCP tool list TTL changed.'
    Assert-True ($summary.protocol_negotiated -eq '2025-11-25') 'MCP handshake failed.'

    $reads = @(Responses 'world_get_components')
    $component = 'bevy_transform::components::transform::Transform'
    Assert-True ($reads.Count -eq 2) 'Expected reads before and after mutation.'
    Assert-True ($reads[0].result.components.$component.translation[0] -eq 0) 'Initial Transform x changed.'
    Assert-True ($reads[1].result.components.$component.translation[0] -eq 42) 'BRP mutation did not persist.'
    Assert-True (@(Responses 'rpc_discover')[0].metadata.method_count -gt 0) 'BRP discovery returned no methods.'

    $active = @(Responses 'brp_list_active_watches')
    $stopped = @(Responses 'brp_stop_watch')
    Assert-True ($active.Count -eq 1 -and $active[0].metadata.watch_count -eq 1) 'Watch did not become active.'
    Assert-True ($stopped.Count -eq 1 -and $stopped[0].status -eq 'success') 'Watch did not stop.'
    Assert-True ($stopped[0].metadata.watch_id -eq $active[0].result[0].watch_id) 'Stopped the wrong watch.'

    $screenshots = @(Responses 'brp_extras_screenshot')
    Assert-True ($screenshots.Count -eq 3) 'Expected primary-window error and two fixture captures.'
    Assert-True ($screenshots[0].status -eq 'error') 'The baseline primary-window screenshot behavior changed.'
    Assert-True ($screenshots[0].call_info.brp_method -eq 'brp_extras/screenshot') 'The primary-window error came from the wrong BRP method.'
    Assert-True ($screenshots[0].metadata.method -eq 'brp_extras/screenshot' -and $screenshots[0].metadata.code -eq -32603) 'The primary-window error contract changed.'
    Assert-True ($screenshots[0].message -match 'Screenshot capture requires a primary window') 'The expected missing-primary-window reason changed.'
    Assert-True ($screenshots[1].result.status -eq 'completed' -and $screenshots[2].result.status -eq 'completed') 'Fixture screenshot did not complete.'
    Assert-True ($screenshots[2].result.rect.width -eq 64 -and $screenshots[2].result.rect.height -eq 48) 'Fixture crop bounds changed.'
    $png = Join-Path $artifacts 'test-app.png'
    $bitmap = [System.Drawing.Bitmap]::new($png)
    try {
        Assert-True ($bitmap.Width -eq 64 -and $bitmap.Height -eq 48) 'Screenshot dimensions changed.'
        Assert-True ($bitmap.GetPixel(8, 8).ToArgb() -eq ([System.Drawing.Color]::Yellow.ToArgb())) 'Screenshot yellow marker is missing.'
        Assert-True ($bitmap.GetPixel(32, 24).ToArgb() -eq ([System.Drawing.Color]::Blue.ToArgb())) 'Screenshot blue region is missing.'
        Assert-True ($bitmap.GetPixel(60, 24).ToArgb() -eq ([System.Drawing.Color]::Magenta.ToArgb())) 'Screenshot magenta region is missing.'
    } finally {
        $bitmap.Dispose()
    }

    $shutdowns = @(Responses 'brp_shutdown')
    Assert-True ($shutdowns.Count -eq 2) 'Expected both apps to shut down.'
    Assert-True (@($shutdowns | Where-Object { $_.metadata.shutdown_method -ne 'clean_shutdown' }).Count -eq 0) 'An app did not shut down cleanly.'
    Assert-True ($summary.runtime_input_chars_queued -eq 8 -and $summary.fixture_input_chars_queued -eq 8) 'Text input was not queued.'
    $logs = @(Responses 'brp_read_log')
    Assert-True ($logs.Count -eq 2) 'App log reads did not complete.'
    Assert-True ($logs[0].result -match 'MARKER:baseline-01') 'The runtime App marker was not captured in its log.'
    Assert-True ($logs[1].result -match 'Screenshot fixtures ready') 'The screenshot fixture readiness marker was not captured in its log.'

    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        if (-not (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) -and
            -not (Get-Process test_app, extras_plugin -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 250
    }
    Assert-True (-not (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)) "Port $Port remains in use."
    Assert-True (-not (Get-Process test_app, extras_plugin -ErrorAction SilentlyContinue)) 'A test app remains running.'
    $passed = $true
} catch {
    $failure = $_
    foreach ($diagnosticFailure in @(Write-FailureArtifacts $failure.Exception.Message)) {
        if ($diagnosticFailure -notin $cleanupFailures) {
            $cleanupFailures += $diagnosticFailure
        }
    }
    throw
} finally {
    if (-not $passed) {
        foreach ($appProcess in @(Get-Process test_app, extras_plugin -ErrorAction SilentlyContinue)) {
            try {
                $appProcess | Stop-Process -Force -ErrorAction Stop
            } catch {
                Write-Warning "Failed to stop test process $($appProcess.Id): $($_.Exception.Message)"
            }
        }
        for ($attempt = 0; $attempt -lt 20; $attempt++) {
            if (-not (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) -and
                -not (Get-Process test_app, extras_plugin -ErrorAction SilentlyContinue)) { break }
            Start-Sleep -Milliseconds 250
        }
        $remainingProcesses = @((Get-Process test_app, extras_plugin -ErrorAction SilentlyContinue)).Count
        $portListening = [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
        if ($remainingProcesses -gt 0 -or $portListening) {
            $originalMessage = if ($null -ne $failure) { $failure.Exception.Message } else { 'unknown regression failure' }
            $cleanupFailures += "App cleanup failed after '$originalMessage': processes=$remainingProcesses, port_listening=$portListening"
        }
    }

    if ($processStarted -and $null -ne $process) {
        try {
            if (-not $process.HasExited -and -not $process.WaitForExit(5000)) {
                $process.Kill()
                if (-not $process.WaitForExit(5000)) {
                    $cleanupFailures += "MCP process $($process.Id) did not exit after termination."
                }
            }
        } catch {
            $cleanupFailures += "Failed while waiting for MCP process exit: $($_.Exception.Message)"
        }
    }
    $remainingMcpProcesses = @((Get-Process bevy_brp_mcp -ErrorAction SilentlyContinue)).Count
    if ($remainingMcpProcesses -gt 0) {
        $cleanupFailures += "MCP cleanup left $remainingMcpProcesses process(es) running."
    } else {
        try {
            if ($traceLogExisted) {
                if (-not (Test-Path -LiteralPath $traceLog)) {
                    throw "Trace log disappeared during the regression: $traceLog"
                }
                $traceStream = [IO.File]::Open($traceLog, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::Read)
                try {
                    $traceStream.SetLength($traceLogLength)
                } finally {
                    $traceStream.Dispose()
                }
            } elseif (Test-Path -LiteralPath $traceLog) {
                Remove-Item -LiteralPath $traceLog -Force
            }
        } catch {
            $cleanupFailures += "Failed to restore the trace log: $($_.Exception.Message)"
        }
    }

    $completedCleanly = $passed -and $cleanupFailures.Count -eq 0
    if ($completedCleanly -and (Test-Path -LiteralPath $artifacts)) {
        if ($KeepArtifacts -or $outputWasRequested) {
            Write-Output "MCP to BRP regression passed. Artifacts retained: $artifacts"
        } else {
            try {
                foreach ($filename in @($summary.runtime_app_log, $summary.fixture_app_log, $summary.watch_log)) {
                    if ($filename -match '^bevy_brp_mcp_(test_app|extras_plugin|watch_).+\.log$') {
                        $log = Join-Path ([IO.Path]::GetTempPath()) $filename
                        if (Test-Path -LiteralPath $log) { Remove-Item -LiteralPath $log -Force }
                    }
                }
                Remove-Item -LiteralPath $artifacts -Recurse -Force
                Write-Output 'MCP to BRP regression passed. Temporary artifacts removed.'
            } catch {
                $cleanupFailures += "Failed to remove regression artifacts or logs: $($_.Exception.Message)"
            }
        }
    }

    $failureMessages = @()
    if ($null -ne $failure) {
        $failureMessages += "Regression failed: $($failure.Exception.Message)"
    }
    $failureMessages += @($cleanupFailures | ForEach-Object { "Cleanup failed: $_" })
    if ($failureMessages.Count -gt 0) {
        $combinedFailure = $failureMessages -join ' '
        $diagnosticDirectoryAvailable = $true
        if (-not (Test-Path -LiteralPath $artifacts)) {
            try {
                [void](New-Item -ItemType Directory -Path $artifacts)
            } catch {
                $directoryFailure = "Failed to create the diagnostic directory: $($_.Exception.Message)"
                if ($directoryFailure -notin $cleanupFailures) {
                    $cleanupFailures += $directoryFailure
                    $combinedFailure += " Cleanup failed: $directoryFailure"
                }
                $diagnosticDirectoryAvailable = $false
            }
        }
        if ($diagnosticDirectoryAvailable) {
            $newDiagnosticFailure = $false
            foreach ($diagnosticFailure in @(Write-FailureArtifacts $combinedFailure)) {
                if ($diagnosticFailure -notin $cleanupFailures) {
                    $cleanupFailures += $diagnosticFailure
                    $combinedFailure += " Cleanup failed: $diagnosticFailure"
                    $newDiagnosticFailure = $true
                }
            }
            if ($newDiagnosticFailure) {
                $summaryFailure = Write-FailureSummary $combinedFailure
                if ($null -ne $summaryFailure -and $summaryFailure -notin $cleanupFailures) {
                    $cleanupFailures += $summaryFailure
                    $combinedFailure += " Cleanup failed: $summaryFailure"
                }
            }
            Write-Output "Regression artifacts retained: $artifacts"
        }
    }
    if ($cleanupFailures.Count -gt 0) {
        throw $combinedFailure
    }
}
