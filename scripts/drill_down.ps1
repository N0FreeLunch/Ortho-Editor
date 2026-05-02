param (
    [Parameter(Mandatory=$true)]
    [string]$FileA,

    [Parameter(Mandatory=$true)]
    [string]$FileB,

    [Parameter(Mandatory=$true)]
    [long]$BitOffset,

    [Parameter(Mandatory=$false)]
    [int]$WindowSize = 64
)

$OutputEncoding = [Console]::InputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# Preferred path: invoke prebuilt binary
$exePath = Join-Path $PSScriptRoot "..\target\debug\d2save_arch.exe"
$args = @("diff", "-a", $FileA, "-b", $FileB, "-o", $BitOffset, "-w", $WindowSize)

if (Test-Path $exePath) {
    & $exePath @args
} else {
    # Fallback: cargo run
    Write-Host "Prebuilt binary not found. Falling back to 'cargo run'..." -ForegroundColor Gray
    $oldCwd = Get-Location
    Set-Location (Join-Path $PSScriptRoot "..")
    cargo run --bin d2save_arch -- @args
    Set-Location $oldCwd
}
