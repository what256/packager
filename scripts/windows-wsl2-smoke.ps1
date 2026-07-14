#requires -Version 7.2

[CmdletBinding()]
param(
  [string]$Packager = "packager",
  [string]$Image = "docker.io/library/nginx:alpine",
  [ValidateRange(60, 3600)]
  [int]$TimeoutSeconds = 600,
  [string]$EvidencePath = "",
  [switch]$KeepGeneratedPackage
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $IsWindows) {
  throw "The managed WSL2 smoke test must run on Windows."
}
if ($Image -notmatch '^[A-Za-z0-9][A-Za-z0-9._/@:-]{0,299}$') {
  throw "Image must be a registry reference without spaces or shell metacharacters."
}

function Invoke-PackagerJson {
  param([Parameter(Mandatory)][string[]]$Arguments)

  $output = (& $Packager --json @Arguments 2>&1 | Out-String).Trim()
  $exitCode = $LASTEXITCODE
  if ($exitCode -ne 0) {
    throw "packager $($Arguments -join ' ') failed with exit code ${exitCode}: $output"
  }
  try {
    return $output | ConvertFrom-Json
  }
  catch {
    throw "packager $($Arguments -join ' ') returned invalid JSON: $output"
  }
}

function Invoke-PackagerQuiet {
  param([Parameter(Mandatory)][string[]]$Arguments)

  try {
    & $Packager @Arguments *> $null
  }
  catch {
    # Cleanup must not hide the primary smoke-test failure.
  }
}

function Get-SmokeApp {
  param([Parameter(Mandatory)][string]$Id)

  $apps = @(Invoke-PackagerJson -Arguments @("apps"))
  return $apps | Where-Object { $_ -and $_.id -eq $Id } | Select-Object -First 1
}

function Wait-SmokeAppReady {
  param(
    [Parameter(Mandatory)][string]$Id,
    [Parameter(Mandatory)][DateTime]$Deadline
  )

  do {
    $app = Get-SmokeApp -Id $Id
    if ($app -and $app.status -eq "ready") {
      return $app
    }
    if ((Get-Date) -ge $Deadline) {
      $status = if ($app) { $app.status } else { "missing" }
      throw "Packaged workload did not become ready before the timeout (last status: $status)."
    }
    Start-Sleep -Seconds 2
  } while ($true)
}

$startedAt = [DateTime]::UtcNow
$id = "packager-wsl2-smoke-$($startedAt.ToString('yyyyMMddHHmmss'))-$PID"
$installed = $false
$runtimeWasRunning = $false
$generatedPackage = $null
$sourcePackage = $null
$result = $null

try {
  $version = (& $Packager --version 2>&1 | Out-String).Trim()
  if ($LASTEXITCODE -ne 0 -or -not $version) {
    throw "Cannot execute Packager at $Packager."
  }

  $initialRuntime = Invoke-PackagerJson -Arguments @("runtime", "status")
  $runtimeWasRunning = [bool]$initialRuntime.running
  $system = Invoke-PackagerJson -Arguments @("status")
  $dataRoot = Split-Path -Parent $system.appDataDir
  $generatedPackage = Join-Path $dataRoot "created-packages/$id"
  $sourcePackage = Join-Path ([IO.Path]::GetTempPath()) "$id-source"
  New-Item -ItemType Directory -Force $sourcePackage | Out-Null
  $compose = @(
    "services:"
    "  web:"
    "    image: $Image"
    "    ports:"
    '      - "80"'
    "    volumes:"
    '      - "${PACKAGER_DATA_DIR:?PACKAGER_DATA_DIR is required}/html:/usr/share/nginx/html:ro"'
    "    restart: unless-stopped"
  ) -join "`n"
  Set-Content -Path (Join-Path $sourcePackage "compose.yml") -Value $compose -Encoding utf8NoBOM

  $built = Invoke-PackagerJson -Arguments @(
    "build", "compose", $sourcePackage,
    "--id", $id,
    "--name", "Packager WSL2 Smoke",
    "--description", "Disposable end-to-end Packager runtime validation workload.",
    "--homepage", "https://nginx.org",
    "--port", "80"
  )
  if ($built.id -ne $id -or $built.status -ne "stopped") {
    throw "Package build/import returned an unexpected result: $($built | ConvertTo-Json -Compress)"
  }
  $installed = $true
  $contentMarker = "Packager WSL2 persistent data $id"
  $contentPattern = [regex]::Escape($contentMarker)
  $htmlDirectory = Join-Path $dataRoot "apps/$id/data/html"
  New-Item -ItemType Directory -Force $htmlDirectory | Out-Null
  Set-Content -Path (Join-Path $htmlDirectory "index.html") -Value $contentMarker -Encoding utf8NoBOM

  $started = Invoke-PackagerJson -Arguments @("start", $id)
  if ($started.id -ne $id -or $started.status -notin @("starting", "ready")) {
    throw "Package start returned an unexpected result: $($started | ConvertTo-Json -Compress)"
  }

  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  $ready = Wait-SmokeAppReady -Id $id -Deadline $deadline
  $response = Invoke-WebRequest -Uri $ready.url -TimeoutSec 20 -NoProxy
  if ($response.StatusCode -ne 200 -or $response.Content -notmatch $contentPattern) {
    throw "Packaged HTTP workload returned an unexpected response from $($ready.url)."
  }

  $logs = Invoke-PackagerJson -Arguments @("logs", $id, "--lines", "100")
  if (-not $logs -or $logs -notmatch "GET / HTTP") {
    throw "Packaged workload logs did not contain the smoke-test request."
  }

  $disabled = Invoke-PackagerJson -Arguments @("auto-updates", "disable", $id)
  if ($disabled.id -ne $id) {
    throw "Disabling automatic updates returned an unexpected result."
  }
  $disabledApp = Get-SmokeApp -Id $id
  if (-not $disabledApp -or $disabledApp.automaticUpdates) {
    throw "Automatic updates were not disabled in shared state."
  }

  $enabled = Invoke-PackagerJson -Arguments @("auto-updates", "enable", $id)
  if ($enabled.id -ne $id) {
    throw "Enabling automatic updates returned an unexpected result."
  }
  $enabledApp = Get-SmokeApp -Id $id
  if (-not $enabledApp -or -not $enabledApp.automaticUpdates) {
    throw "Automatic updates were not enabled in shared state."
  }

  $updated = Invoke-PackagerJson -Arguments @("update", $id)
  if ($updated.id -ne $id -or $updated.status -notin @("starting", "ready")) {
    throw "Package update returned an unexpected result: $($updated | ConvertTo-Json -Compress)"
  }
  $updateDeadline = (Get-Date).AddSeconds($TimeoutSeconds)
  $readyAfterUpdate = Wait-SmokeAppReady -Id $id -Deadline $updateDeadline
  $updatedResponse = Invoke-WebRequest -Uri $readyAfterUpdate.url -TimeoutSec 20 -NoProxy
  if (
    $updatedResponse.StatusCode -ne 200 -or
    $updatedResponse.Content -notmatch $contentPattern
  ) {
    throw "Packaged workload data did not survive its image update."
  }

  $stopped = Invoke-PackagerJson -Arguments @("stop", $id)
  if ($stopped.id -ne $id -or $stopped.status -ne "stopped") {
    throw "Package stop returned an unexpected result: $($stopped | ConvertTo-Json -Compress)"
  }
  $stoppedApp = Get-SmokeApp -Id $id
  if (-not $stoppedApp -or $stoppedApp.status -ne "stopped") {
    throw "Shared app state did not report the workload as stopped."
  }

  $uninstalled = Invoke-PackagerJson -Arguments @("uninstall", $id, "--delete-data")
  if ($uninstalled.id -ne $id -or $uninstalled.status -ne "not-installed") {
    throw "Package uninstall returned an unexpected result: $($uninstalled | ConvertTo-Json -Compress)"
  }
  $installed = $false
  if (Get-SmokeApp -Id $id) {
    throw "The disposable workload remained installed after cleanup."
  }

  $runtime = Invoke-PackagerJson -Arguments @("runtime", "status")
  $result = [ordered]@{
    schemaVersion = 1
    passed = $true
    startedAt = $startedAt.ToString("o")
    completedAt = [DateTime]::UtcNow.ToString("o")
    hostArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    packagerVersion = $version
    image = $Image
    runtimeVersion = $runtime.version
    workloadUrl = $ready.url
    checks = @(
      "package-build-import",
      "managed-runtime-start",
      "compose-pull-up",
      "windows-bind-data-persistence",
      "loopback-http-readiness",
      "container-logs",
      "automatic-update-state",
      "image-update-recreate",
      "stop",
      "destructive-uninstall"
    )
  }
}
finally {
  if ($installed) {
    Invoke-PackagerQuiet -Arguments @("stop", $id)
    Invoke-PackagerQuiet -Arguments @("uninstall", $id, "--delete-data")
  }
  if (-not $KeepGeneratedPackage -and $generatedPackage -and (Test-Path $generatedPackage)) {
    Remove-Item -Recurse -Force $generatedPackage -ErrorAction SilentlyContinue
  }
  if ($sourcePackage -and (Test-Path $sourcePackage)) {
    Remove-Item -Recurse -Force $sourcePackage -ErrorAction SilentlyContinue
  }
  if (-not $runtimeWasRunning) {
    Invoke-PackagerQuiet -Arguments @("runtime", "stop")
  }
}

$evidence = $result | ConvertTo-Json -Depth 5
if ($EvidencePath) {
  $evidenceParent = Split-Path -Parent $EvidencePath
  if ($evidenceParent) {
    New-Item -ItemType Directory -Force $evidenceParent | Out-Null
  }
  Set-Content -Path $EvidencePath -Value $evidence -Encoding utf8NoBOM
}
$evidence
