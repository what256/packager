#requires -Version 7.2

[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Target,
  [Parameter(Mandatory)][string]$ExpectedThumbprint
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $IsWindows) {
  throw "Windows release verification must run on Windows."
}

$expected = $ExpectedThumbprint.Replace(" ", "").ToUpperInvariant()
if ($expected -notmatch '^[0-9A-F]{40,64}$') {
  throw "ExpectedThumbprint is not a certificate thumbprint."
}

$bundleRoot = Join-Path "target/$Target/release" "bundle"
$nsis = @(Get-ChildItem (Join-Path $bundleRoot "nsis") -Filter "*.exe" -File -ErrorAction SilentlyContinue)
$msi = @(Get-ChildItem (Join-Path $bundleRoot "msi") -Filter "*.msi" -File -ErrorAction SilentlyContinue)
if ($nsis.Count -ne 1 -or $msi.Count -ne 1) {
  throw "Expected exactly one NSIS installer and one MSI installer under $bundleRoot."
}

foreach ($installer in @($nsis + $msi)) {
  $signature = Get-AuthenticodeSignature -FilePath $installer.FullName
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
    throw "Invalid Authenticode signature on $($installer.Name): $($signature.StatusMessage)"
  }
  if (-not $signature.SignerCertificate) {
    throw "No Authenticode signer certificate was found on $($installer.Name)."
  }
  $actual = $signature.SignerCertificate.Thumbprint.Replace(" ", "").ToUpperInvariant()
  if ($actual -ne $expected) {
    throw "Unexpected Authenticode signer on $($installer.Name): $actual"
  }
  if (-not $signature.TimeStamperCertificate) {
    throw "The Authenticode signature on $($installer.Name) has no trusted timestamp."
  }

  $updaterSignature = "$($installer.FullName).sig"
  if (-not (Test-Path $updaterSignature -PathType Leaf)) {
    throw "Missing updater signature: $updaterSignature"
  }
  if ((Get-Item $updaterSignature).Length -lt 64) {
    throw "Updater signature is unexpectedly short: $updaterSignature"
  }
}

[ordered]@{
  verified = $true
  target = $Target
  signerThumbprint = $expected
  installers = @($nsis.Name, $msi.Name)
  updaterSignatures = @("$($nsis[0].Name).sig", "$($msi[0].Name).sig")
} | ConvertTo-Json -Depth 3
