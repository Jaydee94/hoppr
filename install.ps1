$ErrorActionPreference = 'Stop'

$Repo       = 'Jaydee94/hoppr'
$InstallDir = Join-Path $env:LOCALAPPDATA 'hoppr\bin'
$BinaryName = 'hoppr.exe'
$BinaryPath = Join-Path $InstallDir $BinaryName

function Get-LatestTag {
    $release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
    $tag = $release.tag_name

    if (-not $tag) {
        Write-Error 'Failed to fetch latest release tag from GitHub API'
        exit 1
    }

    if ($tag -notmatch '^v\d+\.\d+\.\d+') {
        Write-Error "Unexpected tag format: $tag"
        exit 1
    }

    return $tag
}

function Get-InstalledVersion {
    if (Test-Path $BinaryPath) {
        try {
            $output = & $BinaryPath --version 2>$null
            if ($output -match '(\d+\.\d+\.\d+)') { return $Matches[1] }
        } catch {}
    }
    return $null
}

Write-Host 'Fetching latest hoppr release...'

$LatestTag        = Get-LatestTag
$LatestVersion    = $LatestTag.TrimStart('v')
$InstalledVersion = Get-InstalledVersion

if ($InstalledVersion -eq $LatestVersion) {
    Write-Host "Already up to date (v$LatestVersion)"
    exit 0
}

$AssetName   = 'hoppr-windows-x86_64.zip'
$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$AssetName"
$TempDir     = Join-Path $env:TEMP "hoppr-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    Write-Host "Downloading $AssetName..."
    $ZipPath = Join-Path $TempDir $AssetName
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath

    Write-Host 'Extracting...'
    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

    $ExtractedBinary = Get-ChildItem -Path $TempDir -Filter $BinaryName -Recurse |
                       Select-Object -First 1
    if (-not $ExtractedBinary) {
        Write-Error "Binary not found in archive — unexpected archive layout"
        exit 1
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $ExtractedBinary.FullName -Destination $BinaryPath -Force

    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable('PATH', "$UserPath;$InstallDir", 'User')
        Write-Host ''
        Write-Host "Added $InstallDir to your user PATH."
        Write-Host 'Open a new terminal for PATH changes to take effect.'
    }

    Write-Host ''
    Write-Host "hoppr v$LatestVersion installed — run 'hoppr --help' to get started"
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
