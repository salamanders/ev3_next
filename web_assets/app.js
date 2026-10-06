/**
 * LEGO Mindstorms EV3 Web Motor Control - Client Application
 * Zero external JavaScript dependencies.
 */

class EV3App {
    constructor() {
        this.pollIntervalMs = 250;
        this.isPolling = false;
        this.lastLatency = 0;
        this.consecutiveErrors = 0;
        this.activeKeys = new Set();
        this.keyboardDriveActive = false;
        this.driveHeartbeatTimer = null;
        this.currentDriveDir = "stop";

        this.initElements();
        this.attachEventListeners();
        this.startPolling();
    }

    initElements() {
        // Badges
        this.connBadge = document.getElementById("conn-badge");
        this.connText = document.getElementById("conn-text");
        this.latencyVal = document.getElementById("latency-val");
        this.batteryVal = document.getElementById("battery-val");

        // Emergency Stop
        this.btnEstopHeader = document.getElementById("btn-estop-header");

        // Drive elements
        this.driveLeftPort = document.getElementById("drive-left-port");
        this.driveRightPort = document.getElementById("drive-right-port");
        this.driveSpeedSlider = document.getElementById("drive-speed-slider");
        this.driveSpeedVal = document.getElementById("drive-speed-val");
        this.driveSpeedPct = document.getElementById("drive-speed-pct");
        this.dpadButtons = document.querySelectorAll(".btn-dpad");

        // Log box
        this.logBox = document.getElementById("log-box");
        this.btnClearLog = document.getElementById("btn-clear-log");

        // Motor target speed sliders and polarity checkboxes
        this.speedSliders = document.querySelectorAll(".target-speed-slider");
        this.polarityCheckboxes = document.querySelectorAll(".polarity-checkbox");
    }

    attachEventListeners() {
        // Emergency Stop
        this.btnEstopHeader.addEventListener("click", () => this.emergencyStop());

        // Safe Shutdown (BUG-29)
        const btnShutdown = document.getElementById("btn-shutdown-header");
        if (btnShutdown) {
            btnShutdown.addEventListener("click", async () => {
                if (confirm("Are you sure you want to safely power off the EV3 brick?")) {
                    this.log("⚠️ Power off initiated...", "error");
                    try {
                        await this.apiPost("/api/shutdown", {});
                        alert("EV3 shutdown command issued. Power will turn off once filesystem sync completes.");
                    } catch (e) {
                        this.log("Shutdown request failed: " + e.message, "error");
                    }
                }
            });
        }

        // Drive Speed Slider
        this.driveSpeedSlider.addEventListener("input", (e) => {
            const val = e.target.value;
            this.driveSpeedVal.textContent = val;
            this.driveSpeedPct.textContent = Math.round((val / 1050) * 100);
        });

        // D-Pad Direction Buttons: Momentary drive with repeat heartbeat
        this.dpadButtons.forEach(btn => {
            const dir = btn.dataset.dir;
            if (dir === "stop") {
                btn.addEventListener("click", () => this.handleDriveDirection("stop"));
            } else {
                const startDrive = (e) => {
                    e.preventDefault();
                    this.handleDriveDirection(dir);
                };
                const stopDrive = (e) => {
                    e.preventDefault();
                    this.handleDriveDirection("stop");
                };

                btn.addEventListener("pointerdown", startDrive);
                btn.addEventListener("pointerup", stopDrive);
                btn.addEventListener("pointercancel", stopDrive);
                btn.addEventListener("pointerleave", (e) => {
                    if (e.buttons > 0) stopDrive(e);
                });
            }
        });

        // Polarity Inversion Checkboxes
        this.polarityCheckboxes.forEach(cb => {
            const port = cb.dataset.port;
            cb.addEventListener("change", async () => {
                const polarity = cb.checked ? "inversed" : "normal";
                this.log(`[Port ${port}] Setting polarity to ${polarity}`, "info");
                try {
                    await this.apiPost(`/api/motor/${port}/polarity`, { polarity });
                } catch (e) {
                    cb.checked = !cb.checked;
                }
            });
        });

        // Clear Log
        this.btnClearLog.addEventListener("click", () => {
            this.logBox.innerHTML = '<div class="log-entry log-info">[System] Log cleared.</div>';
        });

        // Individual Motor Sliders
        this.speedSliders.forEach(slider => {
            const port = slider.dataset.port;
            const valLabel = document.getElementById(`target-speed-${port}-val`);
            slider.addEventListener("input", (e) => {
                valLabel.textContent = e.target.value;
            });
        });

        // Individual Motor Buttons
        ["A", "B", "C", "D"].forEach(port => {
            const card = document.getElementById(`card-motor-${port}`);
            if (!card) return;

            // Run Forward
            card.querySelector(".btn-run-fwd").addEventListener("click", () => {
                const speed = parseInt(card.querySelector(".target-speed-slider").value, 10);
                this.sendMotorCommand(port, "run-forever", { speed });
            });

            // Run Reverse
            card.querySelector(".btn-run-rev").addEventListener("click", () => {
                const speed = -parseInt(card.querySelector(".target-speed-slider").value, 10);
                this.sendMotorCommand(port, "run-forever", { speed });
            });

            // Coast Stop
            card.querySelector(".btn-stop-coast").addEventListener("click", () => {
                this.sendMotorCommand(port, "stop", { action: "coast" });
            });

            // Brake / Hold Stop
            card.querySelector(".btn-stop-brake").addEventListener("click", () => {
                this.sendMotorCommand(port, "stop", { action: "hold" });
            });

            // Step Buttons (+90, -90, +360)
            card.querySelectorAll(".btn-step").forEach(btn => {
                btn.addEventListener("click", () => {
                    const deg = parseInt(btn.dataset.deg, 10);
                    const speed = parseInt(card.querySelector(".target-speed-slider").value, 10);
                    this.sendMotorCommand(port, "run-to-rel-pos", { speed, position_sp: deg, stop_action: "hold" });
                });
            });

            // Zero Encoder
            card.querySelector(".btn-zero").addEventListener("click", () => {
                this.sendMotorCommand(port, "reset", {});
            });
        });

        // Keyboard Controls
        window.addEventListener("keydown", (e) => this.handleKeyDown(e));
        window.addEventListener("keyup", (e) => this.handleKeyUp(e));

        // Window Blur: prevent sticky keys when user switches tabs
        window.addEventListener("blur", () => {
            if (this.activeKeys.size > 0 || this.keyboardDriveActive || this.currentDriveDir !== "stop") {
                this.activeKeys.clear();
                this.keyboardDriveActive = false;
                this.handleDriveDirection("stop");
            }
        });
    }

    // --- API Calls ---

    async apiPost(endpoint, payload) {
        try {
            const res = await fetch(endpoint, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload)
            });
            const data = await res.json();
            return data;
        } catch (err) {
            this.log(`Error calling ${endpoint}: ${err.message}`, "error");
            throw err;
        }
    }

    async sendMotorCommand(port, action, payload) {
        this.log(`[Port ${port}] Sending ${action} ${JSON.stringify(payload)}`, "info");
        try {
            const data = await this.apiPost(`/api/motor/${port}/${action}`, payload);
            if (data.success) {
                this.log(`[Port ${port}] Success: ${data.data}`, "success");
            } else {
                this.log(`[Port ${port}] Failed: ${data.error}`, "error");
            }
        } catch (e) {
            // Already logged
        }
    }

    async emergencyStop() {
        this.log("⚠️ EMERGENCY STOP TRIGGERED", "error");
        if (this.driveHeartbeatTimer) {
            clearInterval(this.driveHeartbeatTimer);
            this.driveHeartbeatTimer = null;
        }
        this.currentDriveDir = "stop";
        this.keyboardDriveActive = false;
        this.activeKeys.clear();

        try {
            const data = await this.apiPost("/api/emergency-stop", {});
            if (data.success) {
                this.log("All motors halted successfully.", "success");
            }
        } catch (e) {
            this.log("Emergency stop failed: " + e.message, "error");
        }
    }

    async handleDriveDirection(dir) {
        if (dir === "stop") {
            if (this.driveHeartbeatTimer) {
                clearInterval(this.driveHeartbeatTimer);
                this.driveHeartbeatTimer = null;
            }
            this.currentDriveDir = "stop";

            const leftPort = this.driveLeftPort.value;
            const rightPort = this.driveRightPort.value;
            await Promise.allSettled([
                this.sendMotorCommand(leftPort, "stop", { action: "brake" }),
                this.sendMotorCommand(rightPort, "stop", { action: "brake" })
            ]);
            return;
        }

        this.currentDriveDir = dir;
        await this.sendCurrentDrivePacket();

        // Tier 1 Watchdog (BUG-14): Repeat tank-drive packet every 150ms while held
        if (!this.driveHeartbeatTimer) {
            this.driveHeartbeatTimer = setInterval(() => {
                this.sendCurrentDrivePacket();
            }, 150);
        }
    }

    async sendCurrentDrivePacket() {
        if (this.currentDriveDir === "stop") return;

        const speed = parseInt(this.driveSpeedSlider.value, 10);
        const leftPort = this.driveLeftPort.value;
        const rightPort = this.driveRightPort.value;

        let leftSpeed = 0;
        let rightSpeed = 0;

        switch (this.currentDriveDir) {
            case "fwd":
                leftSpeed = speed;
                rightSpeed = speed;
                break;
            case "rev":
                leftSpeed = -speed;
                rightSpeed = -speed;
                break;
            case "spin-left":
                leftSpeed = -speed;
                rightSpeed = speed;
                break;
            case "spin-right":
                leftSpeed = speed;
                rightSpeed = -speed;
                break;
            case "fwd-left":
                leftSpeed = Math.round(speed * 0.4);
                rightSpeed = speed;
                break;
            case "fwd-right":
                leftSpeed = speed;
                rightSpeed = Math.round(speed * 0.4);
                break;
            case "rev-left":
                leftSpeed = -Math.round(speed * 0.4);
                rightSpeed = -speed;
                break;
            case "rev-right":
                leftSpeed = -speed;
                rightSpeed = -Math.round(speed * 0.4);
                break;
            default:
                leftSpeed = 0;
                rightSpeed = 0;
                break;
        }

        try {
            await this.apiPost("/api/tank-drive", {
                left_port: leftPort,
                right_port: rightPort,
                left_speed: leftSpeed,
                right_speed: rightSpeed
            });
        } catch (e) {
            // Heartbeat failed, cancel timer
            if (this.driveHeartbeatTimer) {
                clearInterval(this.driveHeartbeatTimer);
                this.driveHeartbeatTimer = null;
            }
        }
    }

    // --- Keyboard Driving ---

    handleKeyDown(e) {
        if (e.target.tagName === "INPUT" || e.target.tagName === "SELECT") return;

        if (e.code === "Space") {
            e.preventDefault();
            this.emergencyStop();
            return;
        }

        const driveKeys = ["KeyW", "KeyS", "KeyA", "KeyD", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];
        if (driveKeys.includes(e.code) && !this.activeKeys.has(e.code)) {
            e.preventDefault();
            this.activeKeys.add(e.code);
            this.updateKeyboardDrive();
        }
    }

    handleKeyUp(e) {
        if (this.activeKeys.has(e.code)) {
            this.activeKeys.delete(e.code);
            this.updateKeyboardDrive();
        }
    }

    updateKeyboardDrive() {
        if (this.activeKeys.size === 0) {
            if (this.keyboardDriveActive) {
                this.keyboardDriveActive = false;
                this.handleDriveDirection("stop");
            }
            return;
        }

        this.keyboardDriveActive = true;
        const up = this.activeKeys.has("KeyW") || this.activeKeys.has("ArrowUp");
        const down = this.activeKeys.has("KeyS") || this.activeKeys.has("ArrowDown");
        const left = this.activeKeys.has("KeyA") || this.activeKeys.has("ArrowLeft");
        const right = this.activeKeys.has("KeyD") || this.activeKeys.has("ArrowRight");

        if (up && left) this.handleDriveDirection("fwd-left");
        else if (up && right) this.handleDriveDirection("fwd-right");
        else if (down && left) this.handleDriveDirection("rev-left");
        else if (down && right) this.handleDriveDirection("rev-right");
        else if (up) this.handleDriveDirection("fwd");
        else if (down) this.handleDriveDirection("rev");
        else if (left) this.handleDriveDirection("spin-left");
        else if (right) this.handleDriveDirection("spin-right");
    }

    // --- Telemetry Polling Loop ---

    startPolling() {
        if (this.isPolling) return;
        this.isPolling = true;

        const poll = async () => {
            const t0 = performance.now();
            try {
                const res = await fetch("/api/status", { cache: "no-store" });
                if (!res.ok) throw new Error(`HTTP ${res.status}`);
                const data = await res.json();
                const t1 = performance.now();

                this.lastLatency = Math.round(t1 - t0);
                this.consecutiveErrors = 0;
                this.updateConnectionBadge(true);

                if (data.success && data.data) {
                    const motors = Array.isArray(data.data) ? data.data : data.data.motors;
                    if (Array.isArray(motors)) {
                        this.updateMotorCards(motors);
                    }

                    const battery = data.data.battery;
                    if (battery && this.batteryVal) {
                        this.batteryVal.textContent = battery.voltage_v.toFixed(1);
                    }
                }
            } catch (err) {
                this.consecutiveErrors++;
                if (this.consecutiveErrors > 3) {
                    this.updateConnectionBadge(false);
                }
            } finally {
                setTimeout(poll, this.pollIntervalMs);
            }
        };

        poll();
    }

    updateConnectionBadge(isConnected) {
        if (isConnected) {
            this.connBadge.className = "badge badge-connected";
            this.connText.textContent = "Online";
            this.latencyVal.textContent = this.lastLatency;
        } else {
            this.connBadge.className = "badge badge-error";
            this.connText.textContent = "Disconnected";
            this.latencyVal.textContent = "--";
        }
    }

    updateMotorCards(motorList) {
        motorList.forEach(m => {
            const port = m.port;
            const card = document.getElementById(`card-motor-${port}`);
            if (!card) return;

            // Driver name
            const driverElem = document.getElementById(`driver-${port}`);
            if (driverElem) driverElem.textContent = m.driver_name;

            // Polarity Checkbox
            const polarityCb = card.querySelector(".polarity-checkbox");
            if (polarityCb && document.activeElement !== polarityCb) {
                polarityCb.checked = (m.polarity === "inversed");
            }

            // State Badge
            const badge = document.getElementById(`state-badge-${port}`);
            if (badge) {
                if (!m.connected) {
                    badge.className = "motor-state-badge badge-offline";
                    badge.textContent = "Disconnected";
                } else if (m.state.includes("running")) {
                    badge.className = "motor-state-badge badge-running";
                    badge.textContent = "Running";
                } else if (m.state.includes("holding")) {
                    badge.className = "motor-state-badge badge-holding";
                    badge.textContent = "Holding";
                } else {
                    badge.className = "motor-state-badge badge-idle";
                    badge.textContent = "Idle";
                }
            }

            // Speed & RPM
            const speedElem = document.getElementById(`speed-${port}`);
            const rpmElem = document.getElementById(`rpm-${port}`);
            if (speedElem) speedElem.textContent = m.speed;
            if (rpmElem) {
                const rpm = ((m.speed / (m.count_per_rot || 360)) * 60).toFixed(1);
                rpmElem.textContent = rpm;
            }

            // Position & Rotations
            const posElem = document.getElementById(`pos-${port}`);
            const rotElem = document.getElementById(`rot-${port}`);
            if (posElem) posElem.textContent = m.position;
            if (rotElem) {
                const rot = (m.position / (m.count_per_rot || 360)).toFixed(2);
                rotElem.textContent = rot;
            }

            // Duty Cycle Power Bar
            const dutyBar = document.getElementById(`duty-bar-${port}`);
            const dutyVal = document.getElementById(`duty-val-${port}`);
            if (dutyBar && dutyVal) {
                const dutyAbs = Math.abs(m.duty_cycle);
                dutyBar.style.width = `${dutyAbs}%`;
                dutyVal.textContent = m.duty_cycle;
            }
        });
    }

    log(message, type = "info") {
        const time = new Date().toLocaleTimeString();
        const entry = document.createElement("div");
        entry.className = `log-entry log-${type}`;
        entry.textContent = `[${time}] ${message}`;
        this.logBox.appendChild(entry);
        this.logBox.scrollTop = this.logBox.scrollHeight;
    }
}

// Instantiate on load
window.addEventListener("DOMContentLoaded", () => {
    window.app = new EV3App();
});
