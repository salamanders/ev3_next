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

$BuiltWith = ""
if (Get-Command "cargo-zigbuild" -ErrorAction SilentlyContinue) {
    Write-Host "      Using cargo-zigbuild..." -ForegroundColor Gray
    cargo zigbuild --target armv5te-unknown-linux-musleabi --release
    $BuiltWith = "cargo-zigbuild"
} elseif (Get-Command "cross" -ErrorAction SilentlyContinue) {
    Write-Host "      Using cross (Docker)..." -ForegroundColor Gray
    cross build --target armv5te-unknown-linux-musleabi --release
    $BuiltWith = "cross"
} else {
    Write-Host "      Attempting standard cargo build --target armv5te-unknown-linux-musleabi..." -ForegroundColor Gray
    cargo build --target armv5te-unknown-linux-musleabi --release
    $BuiltWith = "cargo"
}

$BinaryPath = "target\armv5te-unknown-linux-musleabi\release\ev3-web-motor"
if (-not (Test-Path $BinaryPath)) {
    Write-Error "[FATAL] Compiled binary not found at $BinaryPath! Please install 'cargo-zigbuild' or 'cross'."
    exit 1
}

$BinarySize = (Get-Item $BinaryPath).Length / 1MB
Write-Host ("      Binary built successfully: {0:N2} MB" -f $BinarySize) -ForegroundColor Green

# 2. Connectivity Check
Write-Host "[2/4] Testing connection to EV3 at $TargetIp..." -ForegroundColor Yellow
$PingSuccess = Test-Connection -ComputerName $TargetIp -Count 1 -Quiet -ErrorAction SilentlyContinue
if (-not $PingSuccess) {
    Write-Warning "Could not ping $TargetIp directly (ICMP might be blocked). Proceeding to SSH test..."
}

# 3. Stop Service & Upload
Write-Host "[3/4] Stopping active service and uploading binary via SCP..." -ForegroundColor Yellow
try {
    # Stop service to prevent 'Text file busy' error
    ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=no "$($User)@$($TargetIp)" "sudo systemctl stop ev3-web.service 2>/dev/null || true"
} catch {
    Write-Warning "Failed to stop service prior to upload (service might not be installed yet)."
}

scp -o StrictHostKeyChecking=no $BinaryPath "$($User)@$($TargetIp):/home/robot/ev3-web-motor"
if ($LASTEXITCODE -ne 0) {
    Write-Error "[FATAL] SCP upload failed. Please verify SSH connectivity and credentials."
    exit 1
}
Write-Host "      Binary uploaded successfully." -ForegroundColor Green

# 4. Service Restart
Write-Host "[4/4] Setting permissions and restarting service..." -ForegroundColor Yellow
ssh -o StrictHostKeyChecking=no "$($User)@$($TargetIp)" "sudo chmod +x /home/robot/ev3-web-motor && sudo systemctl restart ev3-web.service"
if ($LASTEXITCODE -ne 0) {
    Write-Warning "Service restart command failed. Make sure ev3-web.service is installed in /etc/systemd/system/."
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "  SUCCESS! EV3 Web Motor Control is LIVE!                   " -ForegroundColor Green
Write-Host "  Open your browser at: http://$TargetIp/                   " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host ""
