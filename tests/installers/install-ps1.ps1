$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

$repoRoot = (Resolve-Path (Join-Path (Join-Path $PSScriptRoot "..") "..")).Path
$installer = Join-Path $repoRoot "install.ps1"
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("aru-installer-test-" + [Guid]::NewGuid().ToString("N"))
$version = "1.2.3"
$target = "x86_64-pc-windows-msvc"
$archiveName = "aru-$version-$target.zip"
$archiveFixture = Join-Path $temporaryDirectory $archiveName
$checksumsFixture = Join-Path $temporaryDirectory "SHA256SUMS"
$badChecksumsFixture = Join-Path $temporaryDirectory "BAD_SHA256SUMS"
$installDirectory = Join-Path $temporaryDirectory "install"
$oldVersion = $env:ARU_VERSION
$oldInstallDirectory = $env:ARU_INSTALL_DIR
$oldArchitecture = $env:PROCESSOR_ARCHITECTURE
$oldArchitectureW6432 = $env:PROCESSOR_ARCHITEW6432
$global:aruInstallerTestVersion = $version
$global:aruInstallerTestArchiveName = $archiveName
$global:aruInstallerTestArchiveFixture = $archiveFixture
$global:aruInstallerTestChecksumsFixture = $checksumsFixture
$global:aruInstallerTestLatestRequests = 0

function Invoke-RestMethod {
    param(
        [Parameter(Mandatory = $true)] [string] $Uri,
        [hashtable] $Headers
    )
    if ($Uri -ne "https://api.github.com/repos/narumiruna/aru/releases/latest") {
        throw "unexpected API URL: $Uri"
    }
    $global:aruInstallerTestLatestRequests += 1
    return [pscustomobject] @{ tag_name = "v$global:aruInstallerTestVersion" }
}

function Invoke-WebRequest {
    param(
        [Parameter(Mandatory = $true)] [string] $Uri,
        [Parameter(Mandatory = $true)] [string] $OutFile,
        [switch] $UseBasicParsing
    )
    $expectedBaseUrl = "https://github.com/narumiruna/aru/releases/download/v$global:aruInstallerTestVersion"
    if ($Uri -ceq "$expectedBaseUrl/$global:aruInstallerTestArchiveName") {
        Copy-Item -LiteralPath $global:aruInstallerTestArchiveFixture -Destination $OutFile
    }
    elseif ($Uri -ceq "$expectedBaseUrl/SHA256SUMS") {
        Copy-Item -LiteralPath $global:aruInstallerTestChecksumsFixture -Destination $OutFile
    }
    else {
        throw "unexpected download URL: $Uri"
    }
}

try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $payloadDirectory = Join-Path $temporaryDirectory "payload"
    New-Item -ItemType Directory -Path $payloadDirectory | Out-Null
    $payload = [Text.Encoding]::UTF8.GetBytes("aru fixture`r`n")
    $fixtureBinary = Join-Path $payloadDirectory "aru.exe"
    [IO.File]::WriteAllBytes($fixtureBinary, $payload)
    Compress-Archive -LiteralPath $fixtureBinary -DestinationPath $archiveFixture
    $checksum = (Get-FileHash -LiteralPath $archiveFixture -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllBytes($checksumsFixture, [Text.Encoding]::ASCII.GetBytes("$checksum  $archiveName`n"))
    [IO.File]::WriteAllBytes($badChecksumsFixture, [Text.Encoding]::ASCII.GetBytes(("0" * 64) + "  $archiveName`n"))

    Remove-Item Env:ARU_VERSION -ErrorAction SilentlyContinue
    $env:ARU_INSTALL_DIR = $installDirectory
    $env:PROCESSOR_ARCHITECTURE = "AMD64"
    Remove-Item Env:PROCESSOR_ARCHITEW6432 -ErrorAction SilentlyContinue

    & $installer

    $installedBinary = Join-Path $installDirectory "aru.exe"
    if ((Get-FileHash -LiteralPath $fixtureBinary -Algorithm SHA256).Hash -cne (Get-FileHash -LiteralPath $installedBinary -Algorithm SHA256).Hash) {
        throw "installed binary does not match the release fixture"
    }
    if ($global:aruInstallerTestLatestRequests -ne 1) {
        throw "latest release metadata was not requested exactly once"
    }
    Write-Output "PowerShell latest release installation passed"

    $existing = [Text.Encoding]::UTF8.GetBytes("existing binary`r`n")
    [IO.File]::WriteAllBytes($installedBinary, $existing)
    $env:ARU_VERSION = $version
    $global:aruInstallerTestChecksumsFixture = $badChecksumsFixture
    $failed = $false
    try {
        & $installer
    }
    catch {
        $failed = $true
        if ($_.Exception.Message -notmatch "checksum") {
            throw
        }
    }
    if (-not $failed) {
        throw "checksum mismatch unexpectedly succeeded"
    }
    $existingChecksum = [BitConverter]::ToString(([Security.Cryptography.SHA256]::Create()).ComputeHash($existing)).Replace("-", "")
    if ((Get-FileHash -LiteralPath $installedBinary -Algorithm SHA256).Hash -cne $existingChecksum) {
        throw "checksum failure replaced the existing binary"
    }
    if ($global:aruInstallerTestLatestRequests -ne 1) {
        throw "explicit version unexpectedly queried the latest release"
    }
    Write-Output "PowerShell checksum mismatch preserved the existing binary"
}
finally {
    if ($null -eq $oldVersion) { Remove-Item Env:ARU_VERSION -ErrorAction SilentlyContinue } else { $env:ARU_VERSION = $oldVersion }
    if ($null -eq $oldInstallDirectory) { Remove-Item Env:ARU_INSTALL_DIR -ErrorAction SilentlyContinue } else { $env:ARU_INSTALL_DIR = $oldInstallDirectory }
    if ($null -eq $oldArchitecture) { Remove-Item Env:PROCESSOR_ARCHITECTURE -ErrorAction SilentlyContinue } else { $env:PROCESSOR_ARCHITECTURE = $oldArchitecture }
    if ($null -eq $oldArchitectureW6432) { Remove-Item Env:PROCESSOR_ARCHITEW6432 -ErrorAction SilentlyContinue } else { $env:PROCESSOR_ARCHITEW6432 = $oldArchitectureW6432 }
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -Recurse -Force -LiteralPath $temporaryDirectory
    }
    Remove-Variable -Name aruInstallerTestVersion -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name aruInstallerTestArchiveName -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name aruInstallerTestArchiveFixture -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name aruInstallerTestChecksumsFixture -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name aruInstallerTestLatestRequests -Scope Global -ErrorAction SilentlyContinue
}
