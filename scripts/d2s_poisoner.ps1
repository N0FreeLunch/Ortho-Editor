param (
    [Parameter(Mandatory=$true)]
    [string]$InputFile,

    [Parameter(Mandatory=$true)]
    [long]$BitOffset,

    [Parameter(Mandatory=$true)]
    [string]$OutputFile
)

$OutputEncoding = [Console]::InputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# Preferred Path: Use prebuilt binary if available
$scriptDir = Split-Path $MyInvocation.MyCommand.Path -Parent
$repoRoot = Split-Path $scriptDir -Parent
$binPath = Join-Path $repoRoot "target\debug\d2save_poison.exe"

$args = @("-i", "$InputFile", "-b", "$BitOffset", "-o", "$OutputFile")

if (Test-Path $binPath) {
    # Direct invocation
    & $binPath @args
} else {
    # Fallback to cargo run
    Write-Host "Note: Prebuilt binary not found at $binPath. Falling back to 'cargo run'..." -ForegroundColor Cyan
    pushd $repoRoot
    cargo run --bin d2save_poison -- @args
    popd
}
