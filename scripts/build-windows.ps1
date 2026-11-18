param(
    [switch]$SkipInstaller
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $repoRoot
try {
    if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) { throw "Rust/Cargo is required to build the Hliðskjálf command-line tools." }
    cargo build --target x86_64-pc-windows-msvc -p hlidskjalf-desktop -p hlidskjalf-cli -p hlidskjalf-daemon -p hlidskjalf-input-windows --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

    if (-not $SkipInstaller) {
        $nsis = Get-Command "makensis" -ErrorAction SilentlyContinue
        if (-not $nsis) {
            $nsisPath = Join-Path ${env:ProgramFiles(x86)} "NSIS\makensis.exe"
            if (Test-Path $nsisPath) { $nsis = $nsisPath }
        }
        if (-not $nsis) { throw "NSIS is required to build the Windows installer." }
        New-Item -ItemType Directory -Force -Path (Join-Path $repoRoot "dist\windows-native") | Out-Null
        $version = (Select-String -Path (Join-Path $repoRoot "Cargo.toml") -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
        if (-not $version) { throw "Could not read the workspace version from Cargo.toml." }
        & $nsis "/DVERSION=$version" installers\windows\Hlidskjalf.nsi
        if ($LASTEXITCODE -ne 0) { throw "NSIS packaging failed with exit code $LASTEXITCODE" }
        Write-Host "Rust installer generated at dist\windows-native\Hlidskjalf-Setup-Rust-x64.exe"
    }
}
finally {
    Pop-Location
}
