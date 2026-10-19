# Offline installer integration tests; run with Windows PowerShell 5.1 or pwsh.
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot '../install.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) ('twokey-test-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $root
try {
    $fixture = Join-Path $root 'twokey.exe'
    Set-Content -LiteralPath $fixture -Value 'fixture binary'
    $zip = Join-Path $root 'fixture.zip'
    Compress-Archive -LiteralPath $fixture -DestinationPath $zip
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
    $asset = 'twokey-v0.1.0-x86_64-pc-windows-msvc.zip'
    $manifest = "$hash  $asset"
    $destination = Join-Path $root 'install with spaces'
    $modernResponse = $false

    # Intercept downloads, preserving the real hashing, extraction and copying.
    function Invoke-WebRequest {
        param($Uri, $OutFile, $Method, [switch] $UseBasicParsing)
        if ($Method -eq 'Head') {
            if ($modernResponse) {
                return [pscustomobject]@{ BaseResponse = [pscustomobject]@{
                    RequestMessage = [pscustomobject]@{
                        RequestUri = [uri]'https://github.com/test/repo/releases/tag/v0.1.0'
                    }
                } }
            }
            return [pscustomobject]@{ BaseResponse = [pscustomobject]@{
                ResponseUri = [uri]'https://github.com/test/repo/releases/tag/v0.1.0'
            } }
        }
        if ($Uri.EndsWith('/SHA256SUMS')) {
            Set-Content -LiteralPath $OutFile -Value $manifest
        } elseif ($Uri.EndsWith('/' + $asset)) {
            Copy-Item -LiteralPath $zip -Destination $OutFile
        } else { throw "Unexpected download: $Uri" }
    }

    & $installer -Version v0.1.0 -To $destination -Sha256 $hash
    $installed = Join-Path $destination 'twokey.exe'
    if ((Get-FileHash $installed).Hash -ne (Get-FileHash $fixture).Hash) { throw 'Installed bytes differ.' }
    & $installer -Version '' -To $destination -Sha256 $hash
    $modernResponse = $true
    & $installer -Version '' -To $destination -Sha256 $hash

    function Assert-Rejected($Expected, $Pin = '') {
        $before = (Get-FileHash $installed).Hash
        $failure = $null
        try { & $installer -Version v0.1.0 -To $destination -Sha256 $Pin }
        catch { $failure = $_.Exception.Message }
        if (-not $failure -or $failure -notlike "*$Expected*") { throw "Expected '$Expected', got '$failure'." }
        if ((Get-FileHash $installed).Hash -ne $before) { throw 'Failure changed the installed binary.' }
    }
    Assert-Rejected 'Pinned checksum mismatch' ('0' * 64)
    $manifest = ('0' * 64) + "  $asset"
    Assert-Rejected 'Checksum mismatch'
    $manifest = "$hash  another.zip"
    Assert-Rejected 'exactly once'
    $manifest = "$hash  $asset`n$hash  $asset"
    Assert-Rejected 'exactly once'
    $manifest = "$hash *$asset"
    & $installer -Version v0.1.0 -To $destination -Sha256 $hash
    Write-Host 'Installer tests passed.'
} finally {
    # This path is the unique test directory created above.
    Remove-Item -LiteralPath $root -Recurse -Force
}
