# Install aru from a checksum-verified GitHub Release archive.

[CmdletBinding()]
param(
    [string] $Version = $env:ARU_VERSION,
    [string] $InstallDir = $env:ARU_INSTALL_DIR
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$repository = "narumiruna/aru"
$apiUrl = "https://api.github.com/repos/$repository/releases/latest"
$releaseUrl = "https://github.com/$repository/releases/download"
$temporaryDirectory = $null
$stagedBinary = $null
$oldProgressPreference = $ProgressPreference
$oldSecurityProtocol = [Net.ServicePointManager]::SecurityProtocol

function Get-LatestAruVersion {
    $release = Invoke-RestMethod `
        -Uri $apiUrl `
        -Headers @{ Accept = "application/vnd.github+json"; "User-Agent" = "aru-installer" }
    if ($null -eq $release -or [string]::IsNullOrWhiteSpace([string] $release.tag_name)) {
        throw "aru installer: could not determine the latest stable release"
    }
    return [string] $release.tag_name
}

function Save-AruDownload {
    param(
        [Parameter(Mandatory = $true)] [string] $Uri,
        [Parameter(Mandatory = $true)] [string] $Destination
    )

    Invoke-WebRequest -Uri $Uri -OutFile $Destination -UseBasicParsing
}

try {
    $ProgressPreference = "SilentlyContinue"
    [Net.ServicePointManager]::SecurityProtocol = $oldSecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $architecture = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($architecture)) {
        $architecture = $env:PROCESSOR_ARCHITECTURE
    }
    if ($architecture -ne "AMD64") {
        throw "aru installer: unsupported Windows architecture: $architecture"
    }
    $target = "x86_64-pc-windows-msvc"

    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $userProfile = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
        if ([string]::IsNullOrWhiteSpace($userProfile)) {
            throw "aru installer: could not find the user profile; set ARU_INSTALL_DIR explicitly"
        }
        $InstallDir = Join-Path (Join-Path $userProfile ".local") "bin"
    }
    $InstallDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($InstallDir)

    $temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("aru-installer-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null

    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = Get-LatestAruVersion
    }
    if ($Version.StartsWith("v", [StringComparison]::OrdinalIgnoreCase)) {
        $Version = $Version.Substring(1)
    }
    if ($Version -notmatch '^\d+\.\d+\.\d+$') {
        throw "aru installer: ARU_VERSION must match X.Y.Z"
    }

    $archive = "aru-$Version-$target.zip"
    $archivePath = Join-Path $temporaryDirectory $archive
    $checksumsPath = Join-Path $temporaryDirectory "SHA256SUMS"
    $assetUrl = "$releaseUrl/v$Version"
    Save-AruDownload -Uri "$assetUrl/$archive" -Destination $archivePath
    Save-AruDownload -Uri "$assetUrl/SHA256SUMS" -Destination $checksumsPath

    $expectedChecksum = $null
    foreach ($line in Get-Content -LiteralPath $checksumsPath) {
        if ($line -match '^([0-9a-fA-F]{64})[ \t]+\*?(.+)$' -and $Matches[2] -ceq $archive) {
            $expectedChecksum = $Matches[1].ToLowerInvariant()
            break
        }
    }
    if ($null -eq $expectedChecksum) {
        throw "aru installer: no valid checksum found for $archive"
    }
    $actualChecksum = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualChecksum -cne $expectedChecksum) {
        throw "aru installer: checksum verification failed for $archive"
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        $fileEntries = @($zip.Entries | Where-Object { -not [string]::IsNullOrEmpty($_.Name) })
        if ($fileEntries.Count -ne 1 -or $fileEntries[0].FullName -cne "aru.exe") {
            throw "aru installer: release archive has unexpected contents"
        }
    }
    finally {
        $zip.Dispose()
    }

    $extractDirectory = Join-Path $temporaryDirectory "extract"
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDirectory
    $extractedBinary = Join-Path $extractDirectory "aru.exe"
    if (-not (Test-Path -LiteralPath $extractedBinary -PathType Leaf)) {
        throw "aru installer: release archive does not contain aru.exe"
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $destination = Join-Path $InstallDir "aru.exe"
    $stagedBinary = Join-Path $InstallDir (".aru-" + [Guid]::NewGuid().ToString("N") + ".tmp")
    Copy-Item -LiteralPath $extractedBinary -Destination $stagedBinary
    if (Test-Path -LiteralPath $destination -PathType Leaf) {
        [IO.File]::Replace($stagedBinary, $destination, $null)
    }
    else {
        [IO.File]::Move($stagedBinary, $destination)
    }
    $stagedBinary = $null

    Write-Output "Installed aru $Version to $destination"
    if (-not (($env:PATH -split ';') -contains $InstallDir)) {
        Write-Output "Add $InstallDir to your PATH to run aru."
    }
}
finally {
    if ($null -ne $stagedBinary -and (Test-Path -LiteralPath $stagedBinary)) {
        Remove-Item -Force -LiteralPath $stagedBinary
    }
    if ($null -ne $temporaryDirectory -and (Test-Path -LiteralPath $temporaryDirectory)) {
        Remove-Item -Recurse -Force -LiteralPath $temporaryDirectory
    }
    $ProgressPreference = $oldProgressPreference
    [Net.ServicePointManager]::SecurityProtocol = $oldSecurityProtocol
}
