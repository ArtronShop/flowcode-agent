Write-Host " .:: Start ::."

# Get version from package.json
$jsonFile = "package.json"
$packageData = Get-Content $jsonFile -Raw | ConvertFrom-Json
$version = $packageData.version

# ── Download rcedit if not present ────────────────────────────────────────────
$rceditDir = Join-Path $PSScriptRoot "dist"
$rcedit = Join-Path $rceditDir "rcedit.exe"
if (-not (Test-Path $rcedit)) {
    Write-Host " - Downloading rcedit..."
    New-Item -ItemType Directory -Path $rceditDir -Force | Out-Null
    Invoke-WebRequest -Uri "https://github.com/electron/rcedit/releases/download/v2.0.0/rcedit-x64.exe" -OutFile $rcedit -UseBasicParsing
}

# Add rcedit to PATH so pkg can find it when embedding the icon
$env:PATH = "$rceditDir;$env:PATH"

# ── Build exe with pkg (rcedit now in PATH so --icon works) ───────────────────
Write-Host " - Running pkg..."
npx pkg . --icon ./asset/logo.ico
if ($LASTEXITCODE -ne 0) {
    Write-Error "pkg failed"
    exit 1
}

# ── Patch PE subsystem to GUI (2) ─────────────────────────────────────────────
$rawFile = Join-Path $PSScriptRoot "dist\flowcode-agent.exe"
$exeFile = Join-Path $PSScriptRoot "dist\flowcode-agent-v$version.exe"

Write-Host " - Patching subsystem..."
$bytes = [System.IO.File]::ReadAllBytes($rawFile)

# PE header offset at 0x3C
$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)

# Subsystem offset = PE offset + 0x5C  (2 = GUI, 3 = Console)
$subsystemOffset = $peOffset + 0x5C
$bytes[$subsystemOffset] = 2

[System.IO.File]::WriteAllBytes($exeFile, $bytes)

Write-Host "Done. Output: $exeFile"
