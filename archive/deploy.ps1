<#
.SYNOPSIS
    Automated 1-Click Cross-Compilation and Deployment Script for LEGO Mindstorms EV3.
.DESCRIPTION
    Cross-compiles the ev3-web-motor binary for ARMv5te, connects to the EV3 over
    the USB-Ethernet RNDIS interface, uploads the binary, and restarts the systemd daemon.
.PARAMETER TargetIp
    IP address of the EV3 brick (default: 192.168.2.2).
.PARAMETER User
    SSH user on the EV3 (default: robot).
.EXAMPLE
    .\deploy.ps1
    .\deploy.ps1 -TargetIp 192.168.0.105
#>

param (
    [string]$TargetIp = "192.168.2.2",
    [string]$User = "robot"
)

$ErrorActionPreference = "Stop"

Write-Host ""
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   EV3 Web Motor Control - 1-Click Deployment Pipeline       " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Build Phase
Write-Host "[1/4] Cross-compiling for ARMv5te (musl)..." -ForegroundColor Yellow

# Ensure PATH has zig and cargo
if (-not (Get-Command "zig" -ErrorAction SilentlyContinue)) {
    $ZigPkg = Get-ChildItem -Path "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter "zig.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($ZigPkg) {
        $ZigDir = Split-Path $ZigPkg.FullName
        $env:PATH = "$PSScriptRoot;$ZigDir;$env:USERPROFILE\.cargo\bin;$env:PATH"
    } else {
        $env:PATH = "$PSScriptRoot;$env:USERPROFILE\.cargo\bin;$env:PATH"
    }
} else {
    $env:PATH = "$PSScriptRoot;$env:USERPROFILE\.cargo\bin;$env:PATH"
}

Write-Host "      Using standard cargo with Zig LLD linker..." -ForegroundColor Gray
cargo build --target armv5te-unknown-linux-musleabi --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "[FATAL] Cross-compilation failed."
    exit 1
}

$BinaryPath = "target\armv5te-unknown-linux-musleabi\release\ev3-web-motor"
if (-not (Test-Path $BinaryPath)) {
    Write-Error "[FATAL] Compiled binary not found at $BinaryPath!"
    exit 1
}

$BinarySize = (Get-Item $BinaryPath).Length / 1MB
Write-Host ("      Binary built successfully: {0:N2} MB" -f $BinarySize) -ForegroundColor Green

# 2. Connectivity Check
Write-Host "[2/4] Testing connection to EV3 at $TargetIp..." -ForegroundColor Yellow
$PingSuccess = Test-Connection -ComputerName $TargetIp -Count 1 -Quiet -ErrorAction SilentlyContinue
if (-not $PingSuccess) {
    Write-Warning "Could not ping $TargetIp directly (ICMP might be blocked). Proceeding to SSH..."
}

# 3. Upload Phase (Staging in /tmp to prevent 'Text file busy' or interrupted execution)
Write-Host "[3/4] Uploading binary via SCP to /tmp staging..." -ForegroundColor Yellow
scp -o StrictHostKeyChecking=no $BinaryPath "$($User)@$($TargetIp):/tmp/ev3-web-motor.new"
if ($LASTEXITCODE -ne 0) {
    Write-Error "[FATAL] SCP upload failed. Verify network connectivity, target IP, and SSH keys."
    exit 1
}
Write-Host "      Binary uploaded to staging successfully." -ForegroundColor Green

# 4. Atomic Install & Service Restart
Write-Host "[4/4] Installing binary and restarting service..." -ForegroundColor Yellow
ssh -o StrictHostKeyChecking=no "$($User)@$($TargetIp)" "sudo mv /tmp/ev3-web-motor.new /home/robot/ev3-web-motor && sudo chmod +x /home/robot/ev3-web-motor && sudo systemctl restart ev3-web.service"
if ($LASTEXITCODE -ne 0) {
    Write-Error "[FATAL] Failed to install binary or restart ev3-web.service. Verify sudo rules."
    exit 1
}

# 5. Service Status Verification
Write-Host "      Verifying service active status..." -ForegroundColor Gray
ssh -o StrictHostKeyChecking=no "$($User)@$($TargetIp)" "systemctl is-active --quiet ev3-web.service"
if ($LASTEXITCODE -ne 0) {
    Write-Error "[FATAL] ev3-web.service is NOT running. Run 'ssh $($User)@$($TargetIp) journalctl -u ev3-web.service -n 50' to inspect logs."
    exit 1
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "  SUCCESS! EV3 Web Motor Control is LIVE!                   " -ForegroundColor Green
Write-Host "  Open your browser at: http://$TargetIp/                   " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host ""
