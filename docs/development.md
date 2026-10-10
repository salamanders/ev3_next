# EV3 Development & Deployment Guide

This guide describes how to configure the developer environment, simulate hardware on the host computer, cross-compile for ARMv5te, and deploy over USB.

---

## 1. Host Simulation Workflow (Host-First Rule)

Always test user interface, API routes, and state logic on the host computer before deploying to physical hardware.

```bash
# Start simulation on port 8080:
cargo run -- --mock --port 8080

# Open in browser:
http://localhost:8080/
```

### Keyboard Shortcuts:
- <kbd>Spacebar</kbd>: **Emergency Stop (Stops all motors immediately)**

---

## 2. Cross-Compilation Pipeline

The target architecture is `armv5te-unknown-linux-musleabi` targeting the Sitara AM1808 (ARM926EJ-S) processor.

### 1. Install Toolchain Components:
```bash
# Add ARM target to Rust:
rustup target add armv5te-unknown-linux-musleabi

# Install Zig compiler (used as lightweight cross-linker):
# Windows:
winget install -e --id zig.zig
# Linux (Debian/Ubuntu):
sudo apt-get install zig
```

### 2. Cargo Linker Configuration:
The project uses `.cargo/config.toml` to configure `zig-linker.py`:
- Target CPU: `arm926ej-s`
- Release Profile: `opt-level = "z"`, `lto = true`, `strip = true`

### 3. Build Command:
```bash
cargo build --target armv5te-unknown-linux-musleabi --release
```

---

## 3. USB Networking & Driver Setup

Connect the Mini-USB cable between the EV3 PC port and your host computer. The EV3 uses the static IP `192.168.2.2`.

### Verify Connection:
```bash
ping 192.168.2.2
```

### Windows USB Driver Fix (if ping fails):
1. Open Windows **Device Manager** (`Win + X` ➔ `Device Manager`).
2. Right-click `RNDIS/Ethernet Gadget` ➔ **Update driver**.
3. Select **Browse my computer for drivers** ➔ **Let me pick from a list of available drivers on my computer**.
4. Select **Network adapters** ➔ Manufacturer: **Microsoft** ➔ Model: **USB Ethernet/RNDIS Gadget**.
5. Complete installation and verify ping.

### macOS USB Setup (`CDC Composite Gadget`):
1. Open **System Settings ➔ Network**.
2. Select **CDC Composite Gadget** (click `+` to add if not shown).
3. Set IPv4 to **DHCP** or manually configure:
   - IP: `192.168.2.1`
   - Subnet Mask: `255.255.255.0`

---

## 4. One-Time EV3 OS Tuning

Connect over SSH (default user: `robot`, password: `maker`):
```bash
ssh robot@192.168.2.2
```

Run these commands to reduce boot times to **10–15 seconds** and free 20 MB of RAM:

```bash
sudo su

# 1. Disable heavy LCD GUI and reserve /dev/tty1 for console status renderer:
systemctl disable --now brickman.service
systemctl mask getty@tty1.service

# 2. Mask blocking network wait services:
systemctl mask connman-wait-online.service
systemctl mask systemd-networkd-wait-online.service
systemctl mask apt-daily.service apt-daily.timer
systemctl mask apt-daily-upgrade.service apt-daily-upgrade.timer

exit
exit
```

---

## 5. Deployment Scripts (Zero SD-Card Swapping)

Deploy updates over the USB network without removing the MicroSD card:

- **Linux / macOS:**
  ```bash
  ./archive/deploy.sh 192.168.2.2
  ```
- **Windows PowerShell:**
  ```powershell
  .\archive\deploy.ps1 -TargetIp 192.168.2.2
  ```

The deployment script cross-compiles the binary, transfers the file to `/tmp/ev3-web-motor.new`, performs an atomic replace, and restarts `ev3-web.service`.

---

## 6. Running Tests

Run the host test suite:
```bash
cargo test
```
All 35 unit tests (covering motor clamping, sysfs discovery, router endpoints, and asset self-healing) must pass.
