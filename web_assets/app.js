/**
 * LEGO Mindstorms EV3 Web Dashboard Controller
 * Enforces strict single-responsibility boundaries, zero duplication, and ES2025+ standards.
 */

// Restricts numeric magnitude between lower and upper bounds.
const clamp = (v, min, max) => Math.max(min, Math.min(max, v));

class EV3App {
    constructor() {
        this.widgets = [];
        this.hardwareMotors = [];
        this.hardwareSensors = [];
        this.throttleMap = new Map();
        this.isOnline = false;
        this.pollTimer = null;

        this.initStorage();
        this.initDOMElements();
        this.attachGlobalListeners();
        this.startPolling();

        if (this.isDesignView) {
            this.renderHardwarePanel();
            this.rescanHardware(false);
            this.renderDesignWidgets();
        } else {
            this.renderRunWidgets();
        }
    }

    get isDesignView() {
        return !!document.getElementById("design-view");
    }

    // Persists configuration in local memory with fallback defaults.
    initStorage() {
        try {
            const raw = localStorage.getItem("ev3_dashboard_widgets");
            if (raw) {
                const parsed = JSON.parse(raw);
                if (Array.isArray(parsed) && parsed.length > 0) {
                    this.widgets = parsed.map(w => {
                        const base = { id: w.id, type: w.type };
                        if (w.type === "joystick") {
                            return { ...base, leftPort: w.leftPort ?? "B", rightPort: w.rightPort ?? "C" };
                        }
                        if (w.type === "slider") {
                            return { ...base, port: w.port ?? "A" };
                        }
                        if (w.type === "button") {
                            return { ...base, port: w.port ?? "A", action: w.action ?? "momentary", degrees: w.degrees ?? 90 };
                        }
                        if (w.type === "sensor") {
                            return { ...base, port: w.port ?? "1", mode: w.mode ?? "TOUCH" };
                        }
                        return base;
                    });
                    this.saveWidgets();
                    return;
                }
            }
        } catch (_) {}
        this.loadDefaultWidgets();
    }

    loadDefaultWidgets() {
        this.widgets = [
            { id: "joy", type: "joystick", leftPort: "B", rightPort: "C" },
            { id: "slider", type: "slider", port: "A" }
        ];
        this.saveWidgets();
    }

    saveWidgets() {
        try {
            localStorage.setItem("ev3_dashboard_widgets", JSON.stringify(this.widgets));
        } catch (_) {}
    }

    initDOMElements() {
        this.dom = {
            designList: document.getElementById("design-widget-list"),
            runList: document.getElementById("run-widget-list"),
            emptyNotice: document.getElementById("run-empty-notice"),
            hardwareList: document.getElementById("hardware-list"),
            hwCountBadge: document.getElementById("hw-count-badge"),
            widgetSelect: document.getElementById("widget-type-select"),
            btnAdd: document.getElementById("btn-add-widget"),
            btnRescan: document.getElementById("btn-rescan"),
            batteryVal: document.getElementById("battery-val"),
            connBadge: document.getElementById("conn-badge"),
            connText: document.getElementById("conn-text"),
            latencyVal: document.getElementById("latency-val"),
            btnEstop: document.getElementById("btn-estop-header"),
            btnShutdown: document.getElementById("btn-shutdown-header"),
            diagDetails: document.getElementById("diagnostics-details"),
            telemetryBody: document.getElementById("telemetry-table-body"),
            logBox: document.getElementById("log-box"),
            btnClearLog: document.getElementById("btn-clear-log")
        };
    }

    attachGlobalListeners() {
        this.dom.btnEstop?.addEventListener("click", () => this.emergencyStop());
        this.dom.btnShutdown?.addEventListener("click", () => this.powerOff());

        window.addEventListener("keydown", (e) => {
            if (e.code === "Space" && e.target.tagName !== "INPUT") {
                e.preventDefault();
                this.emergencyStop();
            }
        });

        if (this.isDesignView) {
            this.dom.btnAdd?.addEventListener("click", () => {
                const type = this.dom.widgetSelect?.value ?? "joystick";
                this.addWidget(type);
            });
            this.dom.btnRescan?.addEventListener("click", () => this.rescanHardware(true));
            this.dom.btnClearLog?.addEventListener("click", () => {
                if (this.dom.logBox) this.dom.logBox.innerHTML = "";
            });
        }
    }

    // Enforces rate limits with guaranteed delivery of latest trailing action.
    throttle(key, intervalMs, fn) {
        const now = Date.now();
        const entry = this.throttleMap.get(key) ?? { last: 0, timer: null, nextFn: null };
        entry.nextFn = fn;

        if (now - entry.last >= intervalMs) {
            entry.last = now;
            if (entry.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }
            this.throttleMap.set(key, entry);
            fn();
        } else if (!entry.timer) {
            entry.timer = setTimeout(() => {
                entry.last = Date.now();
                entry.timer = null;
                const toRun = entry.nextFn;
                entry.nextFn = null;
                if (toRun) toRun();
            }, intervalMs - (now - entry.last));
            this.throttleMap.set(key, entry);
        }
    }

    // Transmits commands to server and unwraps result data.
    async apiPost(endpoint, payload) {
        const res = await fetch(endpoint, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload),
            signal: AbortSignal.timeout(3000)
        });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json().catch(() => ({}));
    }

    // Immediately halts all active channels.
    async emergencyStop() {
        try {
            await this.apiPost("/api/estop", {});
            document.querySelectorAll(".btn-toggle").forEach(b => {
                b.className = "btn btn-big-action btn-toggle btn-secondary";
                b.textContent = "Start Motor";
            });
            document.querySelectorAll(".run-range-slider").forEach(s => {
                s.value = "0";
                const readout = document.getElementById(`slider-val-${s.dataset.id}`);
                if (readout) readout.textContent = "0";
            });
            this.log("Emergency stop dispatched.", "warn");
        } catch (e) {
            this.log(`Emergency stop failed: ${e.message}`, "error");
        }
    }

    // Requests operating system shutdown.
    async powerOff() {
        if (!confirm("Safely shut down the EV3 brick?")) return;
        try {
            await this.apiPost("/api/shutdown", {});
            alert("EV3 is shutting down. The status light will turn off.");
        } catch (e) {
            alert(`Shutdown request failed: ${e.message}`);
        }
    }

    // Scans ports and updates device inventory tables.
    async rescanHardware(interactive = false) {
        if (this.dom.btnRescan) {
            this.dom.btnRescan.disabled = true;
            this.dom.btnRescan.textContent = "Scanning...";
        }
        try {
            const data = await this.apiPost("/api/rescan", {});
            this.hardwareMotors = data.motors ?? [];
            this.hardwareSensors = data.sensors ?? [];
            this.renderHardwarePanel();
            this.renderDesignWidgets();
            if (interactive) this.log("Hardware scan complete.", "info");
        } catch (e) {
            this.log(`Rescan failed: ${e.message}`, "error");
        } finally {
            if (this.dom.btnRescan) {
                this.dom.btnRescan.disabled = false;
                this.dom.btnRescan.textContent = "Rescan";
            }
        }
    }

    renderHardwarePanel() {
        if (!this.dom.hardwareList) return;
        const portMap = new Map();
        ["A", "B", "C", "D"].forEach(p => portMap.set(p, { type: "motor", name: `Port ${p}`, desc: "Disconnected", connected: false }));
        ["1", "2", "3", "4"].forEach(p => portMap.set(p, { type: "sensor", name: `Port ${p}`, desc: "Disconnected", connected: false }));

        this.hardwareMotors.forEach(m => {
            if (portMap.has(m.port)) {
                portMap.set(m.port, { type: "motor", name: `Port ${m.port}`, desc: m.driver_name || "Motor", connected: m.connected });
            }
        });
        this.hardwareSensors.forEach(s => {
            if (portMap.has(s.port)) {
                portMap.set(s.port, { type: "sensor", name: `Port ${s.port}`, desc: s.driver_name || "Sensor", mode: s.mode, connected: s.connected });
            }
        });

        let connectedCount = 0;
        let html = "";
        for (const [_, item] of portMap) {
            if (item.connected) connectedCount++;
            const badgeClass = item.connected ? "badge-connected" : "badge-disconnected";
            const badgeLabel = item.connected ? (item.mode || "Connected") : "None";
            html += `
                <div class="hw-item ${item.connected ? 'active' : ''}">
                    <div class="hw-item-header">
                        <span class="hw-port-label">${item.name}</span>
                        <span class="badge ${badgeClass}">${badgeLabel}</span>
                    </div>
                    <div class="hw-driver-name">${item.desc}</div>
                </div>
            `;
        }
        this.dom.hardwareList.innerHTML = html;
        if (this.dom.hwCountBadge) {
            this.dom.hwCountBadge.textContent = `${connectedCount} / 8 Connected`;
        }
    }

    portOptions(selected, isSensor = false) {
        const ports = isSensor ? ["1", "2", "3", "4"] : ["A", "B", "C", "D"];
        const list = isSensor ? this.hardwareSensors : this.hardwareMotors;
        return ports.map(p => {
            const hw = list.find(x => x.port === p);
            const label = hw?.connected ? `Port ${p} (${hw.driver_name})` : `Port ${p}`;
            return `<option value="${p}" ${selected === p ? "selected" : ""}>${label}</option>`;
        }).join("");
    }

    addWidget(type) {
        const id = `w_${Date.now()}`;
        const templates = {
            joystick: { id, type: "joystick", leftPort: "B", rightPort: "C" },
            slider: { id, type: "slider", port: "A" },
            button: { id, type: "button", port: "A", action: "momentary", degrees: 90 },
            sensor: { id, type: "sensor", port: "1", mode: "TOUCH" }
        };
        this.widgets.push(templates[type] ?? templates.slider);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    removeWidget(id) {
        this.widgets = this.widgets.filter(w => w.id !== id);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    updateWidget(id, patch) {
        const w = this.widgets.find(x => x.id === id);
        if (w) {
            Object.assign(w, patch);
            this.saveWidgets();
        }
    }

    renderDesignWidgets() {
        if (!this.dom.designList) return;
        const countLabel = document.getElementById("widget-count-label");
        if (countLabel) countLabel.textContent = `${this.widgets.length} active`;

        this.dom.designList.innerHTML = this.widgets.map(w => this.renderDesignCard(w)).join("");
        this.attachDesignCardListeners();
    }

    renderDesignCard(w) {
        const typeLabels = { joystick: "Joystick", slider: "Slider", button: "Button", sensor: "Sensor" };
        let fields = "";

        if (w.type === "joystick") {
            fields = `
                <div class="widget-config-field">
                    <label>Left Motor</label>
                    <select class="field-left-port" data-id="${w.id}">${this.portOptions(w.leftPort)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Right Motor</label>
                    <select class="field-right-port" data-id="${w.id}">${this.portOptions(w.rightPort)}</select>
                </div>
            `;
        } else if (w.type === "slider") {
            fields = `
                <div class="widget-config-field full-width">
                    <label>Motor</label>
                    <select class="field-port" data-id="${w.id}">${this.portOptions(w.port)}</select>
                </div>
            `;
        } else if (w.type === "button") {
            fields = `
                <div class="widget-config-field">
                    <label>Motor</label>
                    <select class="field-port" data-id="${w.id}">${this.portOptions(w.port)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Behavior</label>
                    <select class="field-action" data-id="${w.id}">
                        <option value="momentary" ${w.action === "momentary" ? "selected" : ""}>Hold to Run</option>
                        <option value="toggle" ${w.action === "toggle" ? "selected" : ""}>Toggle (Run / Stop)</option>
                        <option value="step" ${w.action === "step" ? "selected" : ""}>Step Angle</option>
                    </select>
                </div>
                ${w.action === "step" ? `
                    <div class="widget-config-field full-width">
                        <label>Angle (deg)</label>
                        <input type="number" class="field-degrees" data-id="${w.id}" min="-1080" max="1080" step="15" value="${w.degrees}">
                    </div>
                ` : ""}
            `;
        } else if (w.type === "sensor") {
            const sensor = this.hardwareSensors.find(s => s.port === w.port);
            const modes = sensor?.modes?.length ? sensor.modes : ["TOUCH", "COL-COLOR", "US-DIST-CM", "GYRO-ANG"];
            const modeOpts = modes.map(m => `<option value="${m}" ${w.mode === m ? "selected" : ""}>${m}</option>`).join("");
            fields = `
                <div class="widget-config-field">
                    <label>Port</label>
                    <select class="field-sensor-port" data-id="${w.id}">${this.portOptions(w.port, true)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Reading</label>
                    <select class="field-sensor-mode" data-id="${w.id}">${modeOpts}</select>
                </div>
            `;
        }

        return `
            <div class="design-card" id="card-${w.id}">
                <div class="design-card-header">
                    <div class="design-card-title">
                        <span>${typeLabels[w.type] ?? w.type}</span>
                    </div>
                    <button class="btn btn-sm btn-danger btn-remove" data-id="${w.id}">Remove</button>
                </div>
                <div class="widget-config-grid">
                    ${fields}
                </div>
            </div>
        `;
    }

    attachDesignCardListeners() {
        this.dom.designList?.querySelectorAll(".btn-remove").forEach(b => {
            b.addEventListener("click", () => this.removeWidget(b.dataset.id));
        });

        const bindField = (selector, key, parse = v => v) => {
            this.dom.designList?.querySelectorAll(selector).forEach(el => {
                el.addEventListener("change", (e) => {
                    this.updateWidget(e.target.dataset.id, { [key]: parse(e.target.value) });
                    if (selector.includes("action") || selector.includes("sensor-port")) {
                        this.renderDesignWidgets();
                    }
                });
            });
        };

        bindField(".field-left-port", "leftPort");
        bindField(".field-right-port", "rightPort");
        bindField(".field-port", "port");
        bindField(".field-action", "action");
        bindField(".field-degrees", "degrees", Number);
        bindField(".field-sensor-port", "port");
        bindField(".field-sensor-mode", "mode");
    }

    renderRunWidgets() {
        if (!this.dom.runList) return;
        if (this.widgets.length === 0) {
            if (this.dom.emptyNotice) this.dom.emptyNotice.style.display = "block";
            this.dom.runList.innerHTML = "";
            return;
        }

        if (this.dom.emptyNotice) this.dom.emptyNotice.style.display = "none";
        this.dom.runList.innerHTML = this.widgets.map(w => this.renderRunCard(w)).join("");
        this.attachRunCardListeners();
    }

    renderRunCard(w) {
        const sizeClass = w.type === "joystick" ? "run-card-2x2" : "run-card-2x1";
        const portBadge = w.type === "joystick" ? `Ports ${w.leftPort}+${w.rightPort}` : `Port ${w.port}`;
        const typeLabels = { joystick: "Joystick", slider: "Slider", button: "Button", sensor: "Sensor" };
        const cardTitle = typeLabels[w.type] ?? w.type;
        const portAttr = (w.type === "slider" || w.type === "button")
            ? `data-motor-port="${w.port}"`
            : (w.type === "sensor" ? `data-sensor-port="${w.port}"` : "");
        let body = "";

        if (w.type === "joystick") {
            body = `
                <div class="joystick-container">
                    <div class="joystick-boundary" id="joy-boundary-${w.id}">
                        <div class="joystick-crosshair-x"></div>
                        <div class="joystick-crosshair-y"></div>
                        <div class="joystick-knob" id="joy-knob-${w.id}"></div>
                    </div>
                    <div class="joystick-readout" id="joy-readout-${w.id}">X: 0% | Y: 0%</div>
                </div>
            `;
        } else if (w.type === "slider") {
            body = `
                <div class="run-slider-box">
                    <div class="slider-val-row">
                        <span>Speed: <strong class="slider-val-readout" id="slider-val-${w.id}">0</strong> ticks/s</span>
                    </div>
                    <input type="range" class="run-range-slider" id="slider-input-${w.id}"
                           min="-1000" max="1000" value="0" step="25" data-id="${w.id}">
                    <div class="slider-actions">
                        <button class="btn btn-sm btn-secondary btn-slider-zero" data-id="${w.id}">Zero (Stop)</button>
                    </div>
                </div>
            `;
        } else if (w.type === "button") {
            const isToggle = w.action === "toggle";
            const isStep = w.action === "step";
            const btnClass = isToggle ? "btn-secondary btn-toggle" : "btn-primary";
            const label = isToggle
                ? "Start Motor"
                : (isStep ? `Rotate ${w.degrees}°` : "Hold to Run");
            body = `
                <div class="run-button-box">
                    <button class="btn ${btnClass} btn-big-action btn-action-card" id="btn-action-${w.id}" data-id="${w.id}">
                        ${label}
                    </button>
                </div>
            `;
        } else if (w.type === "sensor") {
            body = `
                <div class="run-sensor-box">
                    <div class="sensor-display-val" data-sensor-port="${w.port}" id="sensor-val-${w.id}">--</div>
                    <div class="sensor-display-unit text-muted">${w.mode}</div>
                </div>
            `;
        }

        return `
            <div class="run-card ${sizeClass}" id="run-card-${w.id}" ${portAttr}>
                <div class="run-card-header">
                    <span class="run-card-title">${cardTitle}</span>
                    <span class="badge badge-info">${portBadge}</span>
                </div>
                ${body}
                <div class="run-card-footer text-muted" id="status-card-${w.id}">Ready</div>
            </div>
        `;
    }

    attachRunCardListeners() {
        this.widgets.forEach(w => {
            if (w.type === "joystick") this.initJoystick(w);
            else if (w.type === "slider") this.initSlider(w);
            else if (w.type === "button") this.initButton(w);
        });
    }

    // Maps touch displacement into differential steering velocities.
    initJoystick(w) {
        const boundary = document.getElementById(`joy-boundary-${w.id}`);
        const knob = document.getElementById(`joy-knob-${w.id}`);
        const readout = document.getElementById(`joy-readout-${w.id}`);
        if (!boundary || !knob) return;

        let dragging = false;
        const maxRadius = Math.max(30, (boundary.clientWidth - knob.clientWidth) / 2 || 70);

        const onPointerMove = (clientX, clientY) => {
            const rect = boundary.getBoundingClientRect();
            const rawX = clientX - (rect.left + rect.width / 2);
            const rawY = clientY - (rect.top + rect.height / 2);
            const dist = Math.hypot(rawX, rawY);
            const angle = Math.atan2(rawY, rawX);
            const clampedDist = Math.min(dist, maxRadius);
            const dx = clampedDist * Math.cos(angle);
            const dy = clampedDist * Math.sin(angle);

            knob.style.transform = `translate(${dx}px, ${dy}px)`;

            const normX = dx / maxRadius;
            const normY = -dy / maxRadius; // Upward is positive velocity

            if (readout) {
                readout.textContent = `X: ${Math.round(normX * 100)}% | Y: ${Math.round(normY * 100)}%`;
            }

            this.throttle(`joy_${w.id}`, 66, () => {
                const maxSpeed = 1000;
                const leftSpeed = clamp(Math.round((normY + normX) * maxSpeed), -maxSpeed, maxSpeed);
                const rightSpeed = clamp(Math.round((normY - normX) * maxSpeed), -maxSpeed, maxSpeed);
                this.apiPost("/api/tank-drive", {
                    left_port: w.leftPort,
                    right_port: w.rightPort,
                    left_speed: leftSpeed,
                    right_speed: rightSpeed
                }).catch(() => {});
            });
        };

        const stopMotion = () => {
            if (!dragging) return;
            dragging = false;
            knob.classList.remove("active");
            knob.style.transform = "translate(0px, 0px)";
            if (readout) readout.textContent = "X: 0% | Y: 0%";

            const entry = this.throttleMap.get(`joy_${w.id}`);
            if (entry?.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }

            this.apiPost("/api/tank-drive", {
                left_port: w.leftPort,
                right_port: w.rightPort,
                left_speed: 0,
                right_speed: 0
            }).catch(() => {});
        };

        boundary.addEventListener("pointerdown", (e) => {
            e.preventDefault();
            dragging = true;
            knob.classList.add("active");
            boundary.setPointerCapture(e.pointerId);
            onPointerMove(e.clientX, e.clientY);
        });

        boundary.addEventListener("pointermove", (e) => {
            if (dragging) {
                e.preventDefault();
                onPointerMove(e.clientX, e.clientY);
            }
        });

        boundary.addEventListener("pointerup", stopMotion);
        boundary.addEventListener("pointercancel", stopMotion);
    }

    initSlider(w) {
        const slider = document.getElementById(`slider-input-${w.id}`);
        const readout = document.getElementById(`slider-val-${w.id}`);
        const btnZero = document.querySelector(`.btn-slider-zero[data-id="${w.id}"]`);
        if (!slider) return;

        slider.addEventListener("input", (e) => {
            const speed = parseInt(e.target.value, 10);
            if (readout) readout.textContent = speed;
            this.throttle(`slider_${w.id}`, 66, () => {
                const payload = speed === 0
                    ? { command: "stop", stop_action: "brake" }
                    : { speed };
                this.apiPost(`/api/port/${w.port}`, payload).catch(() => {});
            });
        });

        btnZero?.addEventListener("click", () => {
            slider.value = "0";
            if (readout) readout.textContent = "0";
            const entry = this.throttleMap.get(`slider_${w.id}`);
            if (entry?.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }
            this.apiPost(`/api/port/${w.port}`, { command: "stop", stop_action: "brake" }).catch(() => {});
        });
    }

    // Unifies hold-to-run, toggle, and discrete angle movements.
    initButton(w) {
        const btn = document.getElementById(`btn-action-${w.id}`);
        if (!btn) return;
        const defaultSpeed = 600;

        if (w.action === "momentary") {
            let active = false;
            const start = (e) => {
                e.preventDefault();
                if (active) return;
                active = true;
                btn.classList.add("btn-danger");
                try { btn.setPointerCapture(e.pointerId); } catch (_) {}
                this.apiPost(`/api/port/${w.port}`, { speed: defaultSpeed }).catch(() => {});
            };
            const stop = (e) => {
                e.preventDefault();
                if (!active) return;
                active = false;
                btn.classList.remove("btn-danger");
                this.apiPost(`/api/port/${w.port}`, { command: "stop", stop_action: "brake" }).catch(() => {});
            };
            btn.addEventListener("pointerdown", start);
            btn.addEventListener("pointerup", stop);
            btn.addEventListener("pointercancel", stop);
            btn.addEventListener("pointerleave", (e) => { if (e.buttons > 0) stop(e); });
        } else if (w.action === "toggle") {
            btn.addEventListener("click", () => {
                const isRunning = btn.classList.toggle("toggle-active");
                if (isRunning) {
                    btn.classList.remove("btn-secondary");
                    btn.textContent = "Stop Motor";
                    this.apiPost(`/api/port/${w.port}`, { speed: defaultSpeed }).catch(() => {});
                } else {
                    btn.classList.add("btn-secondary");
                    btn.textContent = "Start Motor";
                    this.apiPost(`/api/port/${w.port}`, { command: "stop", stop_action: "brake" }).catch(() => {});
                }
            });
        } else if (w.action === "step") {
            btn.addEventListener("click", async () => {
                btn.disabled = true;
                const orig = btn.textContent;
                btn.textContent = "Rotating...";
                try {
                    await this.apiPost(`/api/port/${w.port}`, {
                        speed: defaultSpeed,
                        degrees: w.degrees,
                        stop_action: "hold"
                    });
                } finally {
                    setTimeout(() => {
                        btn.disabled = false;
                        btn.textContent = orig;
                    }, 800);
                }
            });
        }
    }

    // Collects periodic system status and updates active elements.
    startPolling() {
        if (this.pollTimer) clearInterval(this.pollTimer);
        this.pollTimer = setInterval(async () => {
            const t0 = performance.now();
            try {
                const res = await fetch("/api/status", { signal: AbortSignal.timeout(2000) });
                if (!res.ok) throw new Error();
                const data = await res.json();
                const rtt = Math.round(performance.now() - t0);

                this.setConnectionState(true, rtt);
                if (this.dom.batteryVal && data.battery_voltage) {
                    this.dom.batteryVal.textContent = data.battery_voltage.toFixed(1);
                }

                if (this.isDesignView && this.dom.diagDetails?.open) {
                    this.renderDiagnosticsTable(data.motors ?? []);
                }

                if (data.sensors) {
                    this.updateSensorReadouts(data.sensors);
                }

                if (data.motors) {
                    this.updateMotorCardStatus(data.motors);
                }
            } catch (_) {
                this.setConnectionState(false, 0);
            }
        }, 250);
    }

    setConnectionState(online, rtt) {
        this.isOnline = online;
        if (this.dom.connBadge && this.dom.connText) {
            this.dom.connBadge.className = `badge ${online ? "badge-connected" : "badge-disconnected"}`;
            this.dom.connText.textContent = online ? "Online" : "Offline";
        }
        if (this.dom.latencyVal) {
            this.dom.latencyVal.textContent = online ? rtt : "--";
        }
    }

    renderDiagnosticsTable(motors) {
        if (!this.dom.telemetryBody) return;
        this.dom.telemetryBody.innerHTML = ["A", "B", "C", "D"].map(port => {
            const m = motors.find(x => x.port === port);
            if (!m) return `<tr><td><strong>${port}</strong></td><td colspan="5" class="text-muted">None</td></tr>`;
            return `
                <tr>
                    <td><strong>${port}</strong></td>
                    <td><span class="badge ${m.state.length ? "badge-running" : "badge-ready"}">${m.state.join(", ") || "Ready"}</span></td>
                    <td>${m.speed}</td>
                    <td>${m.position}</td>
                    <td>${m.duty_cycle}%</td>
                    <td>${m.polarity}</td>
                </tr>
            `;
        }).join("");
    }

    updateSensorReadouts(sensors) {
        sensors.forEach(s => {
            const readout = document.querySelector(`.sensor-display-val[data-sensor-port="${s.port}"]`);
            if (readout && s.value !== undefined) {
                readout.textContent = s.value;
            }
            const cardFooter = document.querySelector(`.run-card[data-sensor-port="${s.port}"] .run-card-footer`);
            if (cardFooter) {
                cardFooter.textContent = s.connected ? `${s.driver_name} (${s.mode})` : "Disconnected";
            }
        });
    }

    updateMotorCardStatus(motors) {
        motors.forEach(m => {
            const cardFooter = document.querySelector(`.run-card[data-motor-port="${m.port}"] .run-card-footer`);
            if (cardFooter) {
                cardFooter.textContent = m.state.length ? m.state.join(", ") : "Ready";
            }
        });
    }

    log(msg, type = "info") {
        if (!this.dom.logBox) return;
        const entry = document.createElement("div");
        entry.className = `log-entry log-${type}`;
        entry.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
        this.dom.logBox.appendChild(entry);
        this.dom.logBox.scrollTop = this.dom.logBox.scrollHeight;
    }
}

// Bootstrap application on page load
window.addEventListener("DOMContentLoaded", () => {
    window.ev3App = new EV3App();
});
