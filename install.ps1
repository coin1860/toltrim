# install.ps1 — install the toltrim binary on Windows.
#
# Detects the architecture, maps it to the release archive the release workflow
# builds for this platform family, downloads it from the project's GitHub
# releases, checks that it really is a non-empty zip, extracts the binary into
# %LOCALAPPDATA%\toltrim\bin, adds that directory to the user PATH when it is not
# already there, and prints exactly what it changed and where.
#
# The binary is staged inside the destination directory and moved into place in
# one step, so a failure never leaves a partial binary behind, and the staging
# directory is removed on every exit path.
#
# Arguments are parsed by hand so that --help and --version work exactly as they
# do in install.sh, without depending on PowerShell parameter binding.
#
#   .\install.ps1 [--version <tag>]
#
# Uninstalling is removing the one directory this script created.

$ErrorActionPreference = 'Stop'

$Repo = if ($env:TOLTRIM_REPO) { $env:TOLTRIM_REPO } else { 'coin1860/toltrim' }
if ($env:TOLTRIM_INSTALL_DIR) {
    $InstallDir = $env:TOLTRIM_INSTALL_DIR
} elseif ($env:LOCALAPPDATA) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'toltrim\bin'
} else {
    $InstallDir = $null
}
$Version = 'latest'

function Fail([string]$Message) {
    [Console]::Error.WriteLine("install.ps1: $Message")
    exit 1
}

function Show-Usage {
    @'
install.ps1: install the toltrim binary on Windows.

USAGE:
    .\install.ps1 [--version <tag>]

OPTIONS:
    --version <tag>   the release tag to install (default: latest)
    -h, --help        show this message and exit

ENVIRONMENT:
    TOLTRIM_INSTALL_DIR   where the binary is installed
                          (default: %LOCALAPPDATA%\toltrim\bin)
    TOLTRIM_REPO          owner/repo to download from (default: coin1860/toltrim)

Uninstalling is removing the one directory this script created.
'@ | Write-Host
}

# --- arguments -------------------------------------------------------------
$arguments = @($args)
for ($i = 0; $i -lt $arguments.Count; $i++) {
    $arg = [string]$arguments[$i]
    if ($arg -eq '-h' -or $arg -eq '--help' -or $arg -eq '-?') {
        Show-Usage
        exit 0
    } elseif ($arg -eq '--version' -or $arg -eq '-Version' -or $arg -eq '-v') {
        if ($i + 1 -ge $arguments.Count) {
            Fail '--version needs a tag, e.g. --version v0.1.0'
        }
        $i++
        $Version = [string]$arguments[$i]
    } elseif ($arg -like '--version=*') {
        $Version = $arg.Substring('--version='.Length)
    } else {
        Fail "unknown argument '$arg'; try --help"
    }
}
if (-not $Version) {
    Fail '--version needs a tag, e.g. --version v0.1.0'
}
if (-not $InstallDir) {
    Fail 'LOCALAPPDATA is not set; set TOLTRIM_INSTALL_DIR to choose where to install'
}

# --- platform --------------------------------------------------------------
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq 'AMD64' -or $arch -eq 'x86_64') {
    $target = 'x86_64-pc-windows-msvc'
} else {
    Fail "unsupported Windows architecture: $arch (the release workflow builds x86_64 only; build from source with cargo instead)"
}

# --- download and install --------------------------------------------------
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('toltrim-install-' + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
    if ($Version -eq 'latest') {
        try {
            # No -UseBasicParsing here: Invoke-RestMethod never had that
            # parameter, on Windows PowerShell or on PowerShell 7.
            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
        } catch {
            Fail "could not read the latest release tag from $Repo"
        }
        $tag = [string]$release.tag_name
        if (-not $tag) { Fail "could not read the latest release tag from $Repo" }
    } else {
        $tag = $Version
        if (-not $tag.StartsWith('v')) { $tag = "v$tag" }
    }
    $ver = if ($tag.StartsWith('v')) { $tag.Substring(1) } else { $tag }

    $url = "https://github.com/$Repo/releases/download/$tag/toltrim-$ver-$target.zip"
    $archive = Join-Path $tmp "toltrim-$ver-$target.zip"
    try {
        Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
    } catch {
        Fail "download failed: $url"
    }
    if (-not (Test-Path $archive) -or (Get-Item $archive).Length -eq 0) {
        Fail "the downloaded archive is empty: $url"
    }

    $extract = Join-Path $tmp 'extracted'
    try {
        Expand-Archive -Path $archive -DestinationPath $extract -Force
    } catch {
        Fail "the download is not a readable zip archive: $url"
    }

    $binary = Join-Path $extract "toltrim-$ver-$target\toltrim.exe"
    if (-not (Test-Path $binary)) {
        $found = Get-ChildItem -Path $extract -Filter 'toltrim.exe' -Recurse -File | Select-Object -First 1
        if ($found) { $binary = $found.FullName } else { Fail "the archive does not contain a toltrim binary: $url" }
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $final = Join-Path $InstallDir 'toltrim.exe'
    $staged = Join-Path $InstallDir ('.toltrim.staged.' + $PID + '.exe')
    try {
        Copy-Item -Path $binary -Destination $staged -Force
        Move-Item -Path $staged -Destination $final -Force
    } catch {
        Remove-Item -Path $staged -Force -ErrorAction SilentlyContinue
        Fail "cannot install into ${InstallDir}: $($_.Exception.Message)"
    }

    Write-Host "installed toltrim $ver for $target"
    Write-Host "  from $url"
    Write-Host "  to   $final"

    $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if (-not $userPath) { $userPath = '' }
    $entries = @($userPath -split ';' | Where-Object { $_ -ne '' })
    if ($entries -notcontains $InstallDir) {
        $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        Write-Host "added $InstallDir to the user PATH; new terminals will find toltrim"
    } else {
        Write-Host "$InstallDir is already on the user PATH"
    }

    Write-Host 'to uninstall, remove that one directory:'
    Write-Host "  Remove-Item -Recurse -Force '$InstallDir'"
} finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
