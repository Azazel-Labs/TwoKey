#Requires -Version 5.1
<#
.SYNOPSIS
Install the Windows x64 twokey binary from GitHub Releases without Rust.
.DESCRIPTION
Verifies SHA256SUMS and an optional pinned archive checksum before installation.
Defaults to ~/.local/bin. Add that directory to PATH to run twokey by name.
Windows ARM64 users can use the x64 binary through Windows x64 emulation.
.EXAMPLE
./install.ps1 -Version v0.1.0 -To "$HOME/.local/bin" -Sha256 <hex>
#>
[CmdletBinding()]
param(
    [string] $Version = $env:TWOKEY_VERSION,
    [string] $To = $env:TWOKEY_INSTALL_DIR,
    [string] $Sha256 = $env:TWOKEY_SHA256,
    [string] $Repo = $env:TWOKEY_REPO,
    [string] $GitHub = $env:TWOKEY_GITHUB
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on Linux or macOS.' }
if (-not [Environment]::Is64BitOperatingSystem) { throw 'twokey requires 64-bit Windows.' }
if (-not $To) { $To = Join-Path $HOME '.local/bin' }
if (-not $Repo) { $Repo = 'Azazel-Labs/twokey' }
if (-not $GitHub) { $GitHub = 'https://github.com' }
if ($Sha256 -and $Sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Sha256 must be 64 hexadecimal characters.' }
$GitHub = $GitHub.TrimEnd('/')
# Windows PowerShell 5.1 may otherwise default to older TLS protocols.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

if (-not $Version) {
    $response = Invoke-WebRequest -UseBasicParsing -Uri "$GitHub/$Repo/releases/latest" -Method Head
    # Windows PowerShell and PowerShell 7 expose the final redirect differently.
    if ($response.BaseResponse.PSObject.Properties['ResponseUri']) {
        $releaseUri = $response.BaseResponse.ResponseUri
    } else {
        $releaseUri = $response.BaseResponse.RequestMessage.RequestUri
    }
    $Version = $releaseUri.AbsolutePath.TrimEnd('/').Split('/')[-1]
}
if ($Version -notmatch '^v[0-9][0-9A-Za-z.+-]*$') { throw 'Cannot determine release tag; pass -Version vX.Y.Z.' }

$asset = "twokey-$Version-x86_64-pc-windows-msvc.zip"
$base = "$GitHub/$Repo/releases/download/$Version"
$temp = Join-Path ([IO.Path]::GetTempPath()) ("twokey-" + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $temp
try {
    Write-Host "Downloading $asset ($Repo $Version)"
    $archive = Join-Path $temp $asset
    $checksums = Join-Path $temp 'SHA256SUMS'
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $archive
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $checksums
    $entries = @(Get-Content -LiteralPath $checksums | Where-Object {
        $_ -cmatch ('^[a-fA-F0-9]{64}\s+\*?' + [regex]::Escape($asset) + '$')
    })
    if ($entries.Count -ne 1) { throw "$asset must appear exactly once in SHA256SUMS." }
    $expected = ($entries[0] -split '\s+')[0]
    $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "Checksum mismatch for $asset." }
    if ($Sha256 -and $actual -ne $Sha256) { throw "Pinned checksum mismatch for $asset." }
    $expanded = Join-Path $temp 'expanded'
    Expand-Archive -LiteralPath $archive -DestinationPath $expanded
    $binary = Join-Path $expanded 'twokey.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Archive does not contain twokey.exe.' }
    $null = New-Item -ItemType Directory -Path $To -Force
    $destination = Join-Path (Resolve-Path -LiteralPath $To).Path 'twokey.exe'
    Copy-Item -LiteralPath $binary -Destination $destination -Force
    Write-Host "Installed $Version to $destination"
    Write-Host "Add $((Resolve-Path -LiteralPath $To).Path) to your user PATH if needed."
} finally {
    # Only remove the unique temporary directory created by this invocation.
    Remove-Item -LiteralPath $temp -Recurse -Force
}
