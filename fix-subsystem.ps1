$file = "dist/flowcode-agent.exe"
$bytes = [System.IO.File]::ReadAllBytes($file)

# PE header offset อยู่ที่ 0x3C
$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)

# Subsystem offset = PE offset + 0x5C
$subsystemOffset = $peOffset + 0x5C

# 2 = GUI, 3 = Console
$bytes[$subsystemOffset] = 2

[System.IO.File]::WriteAllBytes($file, $bytes)
Write-Host "Done! Subsystem changed to GUI (windowless)"
