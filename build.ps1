# Build flowcode-agent and output versioned exe to build/

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
            Select-Object -First 1).Matches.Groups[1].Value

if (-not $version) {
    Write-Error "Could not read version from Cargo.toml"
    exit 1
}

$out = "build\flowcode-agent-V$version.exe"

Write-Host "Building v$version..."
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (-not (Test-Path "build")) {
    New-Item -ItemType Directory -Path "build" | Out-Null
}

Copy-Item "target\release\flowcode-agent.exe" $out -Force
Write-Host "Output: $out"
