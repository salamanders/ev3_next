/**
 * LEGO Mindstorms EV3 Web Dashboard Controller
 * Enforces strict single-responsibility boundaries, zero duplication, and ES2025+ standards.
 */

// Translates component category keys to presentation title strings.
const TYPE_LABELS = { joystick: "Joystick", slider: "Slider", button: "Button", sensor: "Sensor" };

// Restricts numeric magnitude between lower and upper bounds.
const clamp = (v, min, max) => Math.max(min, Math.min(max, v));

// Extracts unique active actuator sockets assigned to directional axes.
function getJoystickPorts(w) {
    return !w.multiMotor
        ? [w.xPort, w.yPort]
        : [...new Set([...(w.xMotors ?? []).map(m => m.port), ...(w.yMotors ?? []).map(m => m.port)])];
}

// Maps planar displacement vectors to multi-channel velocity setpoints.
function calculateJoystickCommands(w, normX, normY, maxSpeed = 1000) {
    const stopAction = w.stopAction || "brake";
    const xSpeed = clamp(Math.round(normX * maxSpeed), -maxSpeed, maxSpeed);
    const ySpeed = clamp(Math.round(normY * maxSpeed), -maxSpeed, maxSpeed);

    if (!w.multiMotor) {
        return [
            { port: w.xPort, speed: xSpeed, stop_action: stopAction },
            { port: w.yPort, speed: ySpeed, stop_action: stopAction }
        ];
    }

    const mapMotors = (motors, speed) => (motors ?? []).map(m => ({
        port: m.port,
        speed: m.invert ? -speed : speed,
        stop_action: stopAction
    }));
    return [...mapMotors(w.xMotors, xSpeed), ...mapMotors(w.yMotors, ySpeed)];
}

// Generates zero-velocity brake targets across all configured channels.
function getJoystickStopCommands(w) {
    const stopAction = w.stopAction || "brake";
    return getJoystickPorts(w).map(port => ({ port, speed: 0, stop_action: stopAction }));
}

class EV3App {
    // Initializes client state and activates primary subsystem pipelines.
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

    // Distinguishes configuration editing mode from runtime operations.
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
                    const parseMotors = (motors, fallbackPort) => Array.isArray(motors)
                        ? motors.map(m => ({ port: String(m.port).trim().toUpperCase(), invert: !!m.invert }))
                        : [{ port: fallbackPort, invert: false }];

                    this.widgets = parsed.map(w => {
                        const base = { id: w.id, type: w.type };
                        if (w.type === "joystick") {
                            const xPort = String(w.xPort ?? w.leftPort ?? "B").trim().toUpperCase();
                            const yPort = String(w.yPort ?? w.rightPort ?? "C").trim().toUpperCase();
                            return {
                                ...base,
                                xPort,
                                yPort,
                                stopAction: w.stopAction ?? "brake",
                                multiMotor: !!w.multiMotor,
                                xMotors: parseMotors(w.xMotors, xPort),
                                yMotors: parseMotors(w.yMotors, yPort)
                            };
                        }
                        if (w.type === "slider") return { ...base, port: w.port ?? "A" };
                        if (w.type === "button") return { ...base, port: w.port ?? "A", action: w.action ?? "momentary", degrees: w.degrees ?? 90 };
                        if (w.type === "sensor") return { ...base, port: w.port ?? "1", mode: w.mode ?? "TOUCH" };
                        return base;
                    });
                    this.saveWidgets();
                    return;
                }
            }
        } catch (_) {}
        this.loadDefaultWidgets();
    }

    // Seeds initial interface layout when persistent storage is absent.
    loadDefaultWidgets() {
        this.widgets = [
            {
                id: "joy",
                type: "joystick",
                xPort: "B",
                yPort: "C",
                stopAction: "brake",
                multiMotor: false,
                xMotors: [{ port: "B", invert: false }],
                yMotors: [{ port: "C", invert: false }]
            },
            { id: "slider", type: "slider", port: "A" }
        ];
        this.saveWidgets();
    }

    // Commits current UI configurations to browser storage for persistence.
    saveWidgets() {
        try {
            localStorage.setItem("ev3_dashboard_widgets", JSON.stringify(this.widgets));
        } catch (_) {}
    }

    // Caches page markup references to avoid repeated document lookups.
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

    // Registers top-level keyboard and header button event bindings.
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
        const json = await res.json().catch(() => ({}));
        return json?.data !== undefined ? json.data : json;
    }

    // Dispatches a multi-port command batch with heartbeat safety watchdog.
    async apiBatch(commands, heartbeatMs = 400) {
        return this.apiPost("/api/batch", {
            heartbeat_ms: heartbeatMs,
            commands
        });
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
            const res = await this.apiPost("/api/rescan", {});
            const data = res?.data ?? res;
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

    // Displays connected peripheral inventory with live link indicators.
    renderHardwarePanel() {
        if (!this.dom.hardwareList) return;
        const portMap = new Map([
            ...["A", "B", "C", "D"].map(p => [p, { type: "motor", name: `Port ${p}`, desc: "Disconnected", connected: false }]),
            ...["1", "2", "3", "4"].map(p => [p, { type: "sensor", name: `Port ${p}`, desc: "Disconnected", connected: false }])
        ]);

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
        for (const [, item] of portMap) {
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

    // Builds selection dropdown entries matching available device categories.
    portOptions(selected, isSensor = false) {
        const ports = isSensor ? ["1", "2", "3", "4"] : ["A", "B", "C", "D"];
        const list = isSensor ? this.hardwareSensors : this.hardwareMotors;
        return ports.map(p => {
            const hw = list.find(x => x.port === p);
            const label = hw?.connected ? `Port ${p} (${hw.driver_name})` : `Port ${p}`;
            return `<option value="${p}" ${selected === p ? "selected" : ""}>${label}</option>`;
        }).join("");
    }

    // Appends a new item schema and triggers canvas refresh.
    addWidget(type) {
        const id = `w_${Date.now()}`;
        const templates = {
            joystick: {
                id,
                type: "joystick",
                xPort: "B",
                yPort: "C",
                stopAction: "brake",
                multiMotor: false,
                xMotors: [{ port: "B", invert: false }],
                yMotors: [{ port: "C", invert: false }]
            },
            slider: { id, type: "slider", port: "A" },
            button: { id, type: "button", port: "A", action: "momentary", degrees: 90 },
            sensor: { id, type: "sensor", port: "1", mode: "TOUCH" }
        };
        this.widgets.push(templates[type] ?? templates.slider);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    // Deletes a configured item and updates the layout canvas.
    removeWidget(id) {
        this.widgets = this.widgets.filter(w => w.id !== id);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    // Modifies specific properties on an item without recreating it.
    updateWidget(id, patch) {
        const w = this.widgets.find(x => x.id === id);
        if (w) {
            Object.assign(w, patch);
            this.saveWidgets();
        }
    }

    // Populates the setup container with editable cards and listeners.
    renderDesignWidgets() {
        if (!this.dom.designList) return;
        const countLabel = document.getElementById("widget-count-label");
        if (countLabel) countLabel.textContent = `${this.widgets.length} active`;

        this.dom.designList.innerHTML = this.widgets.map(w => this.renderDesignCard(w)).join("");
        this.attachDesignCardListeners();
    }

    // Formats editable inputs and settings markup for an item.
    renderDesignCard(w) {
        let fields = "";

        if (w.type === "joystick") {
            const motorCols = ["A", "B", "C", "D"];
            const axes = [
                { axis: "x", label: "X Axis (Left / Right)", motors: w.xMotors },
                { axis: "y", label: "Y Axis (Up / Down)", motors: w.yMotors }
            ];
            const multiMotorContent = `
                <div class="multi-motor-container">
                    ${axes.map(({ axis, label, motors }) => `
                        <div class="multi-motor-column">
                            <span class="multi-motor-heading">${label}</span>
                            <div class="multi-motor-grid">
                                ${motorCols.map(p => {
                                    const assigned = motors?.find(m => m.port === p);
                                    const isChecked = !!assigned;
                                    const isInverted = !!assigned?.invert;
                                    return `
                                        <div class="multi-motor-row">
                                            <label class="multi-motor-label">
                                                <input type="checkbox" class="field-mm-active" data-id="${w.id}" data-axis="${axis}" data-port="${p}" ${isChecked ? "checked" : ""}>
                                                <span>Port ${p}</span>
                                            </label>
                                            <label class="multi-motor-invert-label ${isChecked ? "" : "disabled"}">
                                                <input type="checkbox" class="field-mm-invert" data-id="${w.id}" data-axis="${axis}" data-port="${p}" ${isInverted ? "checked" : ""} ${isChecked ? "" : "disabled"}>
                                                <span>Invert</span>
                                            </label>
                                        </div>
                                    `;
                                }).join("")}
                            </div>
                        </div>
                    `).join("")}
                </div>
            `;

            const standardContent = `
                <div class="widget-config-field">
                    <label>X Axis</label>
                    <select class="field-x-port" data-id="${w.id}">${this.portOptions(w.xPort)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Y Axis</label>
                    <select class="field-y-port" data-id="${w.id}">${this.portOptions(w.yPort)}</select>
                </div>
            `;

            fields = `
                <div class="widget-config-field full-width">
                    <label class="multi-motor-toggle">
                        <input type="checkbox" class="field-multi-motor" data-id="${w.id}" ${w.multiMotor ? "checked" : ""}>
                        <span>Multi-Motor Control</span>
                    </label>
                </div>
                ${w.multiMotor ? multiMotorContent : standardContent}
                <div class="widget-config-field full-width">
                    <label>Release Action</label>
                    <select class="field-stop-action" data-id="${w.id}">
                        <option value="brake" ${w.stopAction === "brake" ? "selected" : ""}>Brake (Hold Position)</option>
                        <option value="coast" ${w.stopAction === "coast" ? "selected" : ""}>Coast (Free Spin)</option>
                    </select>
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
                        <span>${TYPE_LABELS[w.type] ?? w.type}</span>
                    </div>
                    <button class="btn btn-sm btn-danger btn-remove" data-id="${w.id}">Remove</button>
                </div>
                <div class="widget-config-grid">
                    ${fields}
                </div>
            </div>
        `;
    }

    // Connects change events on configuration inputs to schema mutators.
    attachDesignCardListeners() {
        this.dom.designList?.querySelectorAll(".btn-remove").forEach(b => {
            b.addEventListener("click", () => this.removeWidget(b.dataset.id));
        });

        const bindField = (selector, key, parse = v => v) => {
            this.dom.designList?.querySelectorAll(selector).forEach(el => {
                el.addEventListener("change", (e) => {
                    this.updateWidget(e.target.dataset.id, { [key]: parse(e.target.value) });
                    if (selector === ".field-action" || selector === ".field-sensor-port") {
                        this.renderDesignWidgets();
                    }
                });
            });
        };

        bindField(".field-stop-action", "stopAction");
        bindField(".field-port", "port");
        bindField(".field-action", "action");
        bindField(".field-degrees", "degrees", Number);
        bindField(".field-sensor-port", "port");

        this.dom.designList?.querySelectorAll(".field-sensor-mode").forEach(el => {
            el.addEventListener("change", (e) => {
                const { id } = e.target.dataset;
                const mode = e.target.value;
                this.updateWidget(id, { mode });
                const w = this.widgets.find(x => x.id === id);
                if (w?.port) {
                    this.apiPost(`/api/port/${w.port}`, { mode }).catch(err => {
                        this.log(`Failed to set sensor mode: ${err.message}`, "error");
                    });
                }
            });
        });

        [".field-x-port", ".field-y-port"].forEach(selector => {
            const axis = selector.includes("x") ? "x" : "y";
            this.dom.designList?.querySelectorAll(selector).forEach(el => {
                el.addEventListener("change", (e) => {
                    const { id } = e.target.dataset;
                    const port = e.target.value;
                    const w = this.widgets.find(x => x.id === id);
                    const patch = { [`${axis}Port`]: port };
                    if (w && !w.multiMotor) {
                        patch[`${axis}Motors`] = [{ port, invert: false }];
                    }
                    this.updateWidget(id, patch);
                });
            });
        });

        this.dom.designList?.querySelectorAll(".field-multi-motor").forEach(el => {
            el.addEventListener("change", (e) => {
                const id = e.target.dataset.id;
                const enabled = e.target.checked;
                const w = this.widgets.find(x => x.id === id);
                if (w) {
                    const patch = { multiMotor: enabled };
                    if (enabled) {
                        patch.xMotors = w.xMotors?.length ? w.xMotors : [{ port: w.xPort ?? "B", invert: false }];
                        patch.yMotors = w.yMotors?.length ? w.yMotors : [{ port: w.yPort ?? "C", invert: false }];
                    }
                    this.updateWidget(id, patch);
                    this.renderDesignWidgets();
                }
            });
        });

        this.dom.designList?.querySelectorAll(".field-mm-active").forEach(el => {
            el.addEventListener("change", (e) => {
                const { id, axis, port } = e.target.dataset;
                const w = this.widgets.find(x => x.id === id);
                if (!w) return;
                const key = axis === "x" ? "xMotors" : "yMotors";
                const list = [...(w[key] ?? [])];
                const idx = list.findIndex(m => m.port === port);
                const isActive = e.target.checked;
                const patch = {};
                if (isActive) {
                    if (idx === -1) list.push({ port, invert: false });
                    // Remove from opposite axis so one motor is assigned to only one axis
                    const oppKey = axis === "x" ? "yMotors" : "xMotors";
                    const oppList = (w[oppKey] ?? []).filter(m => m.port !== port);
                    patch[oppKey] = oppList;

                    const oppCheckbox = el.closest(".multi-motor-container")?.querySelector(`.field-mm-active[data-axis="${axis === "x" ? "y" : "x"}"][data-port="${port}"]`);
                    if (oppCheckbox && oppCheckbox.checked) {
                        oppCheckbox.checked = false;
                        const oppRow = oppCheckbox.closest(".multi-motor-row");
                        const oppInvert = oppRow?.querySelector(".field-mm-invert");
                        const oppInvertLabel = oppRow?.querySelector(".multi-motor-invert-label");
                        if (oppInvert) { oppInvert.checked = false; oppInvert.disabled = true; }
                        if (oppInvertLabel) oppInvertLabel.classList.add("disabled");
                    }
                } else {
                    if (idx !== -1) list.splice(idx, 1);
                }
                patch[key] = list;
                if (list.length > 0) {
                    if (axis === "x") patch.xPort = list[0].port;
                    else patch.yPort = list[0].port;
                }
                this.updateWidget(id, patch);

                const row = el.closest(".multi-motor-row");
                const invertInput = row?.querySelector(".field-mm-invert");
                const invertLabel = row?.querySelector(".multi-motor-invert-label");
                if (invertInput && invertLabel) {
                    invertInput.disabled = !isActive;
                    if (!isActive) invertInput.checked = false;
                    invertLabel.classList.toggle("disabled", !isActive);
                }
            });
        });

        this.dom.designList?.querySelectorAll(".field-mm-invert").forEach(el => {
            el.addEventListener("change", (e) => {
                const { id, axis, port } = e.target.dataset;
                const w = this.widgets.find(x => x.id === id);
                if (!w) return;
                const key = axis === "x" ? "xMotors" : "yMotors";
                const list = (w[key] ?? []).map(m => m.port === port ? { ...m, invert: e.target.checked } : m);
                this.updateWidget(id, { [key]: list });
            });
        });
    }

    // Displays live operator controls and attaches input handlers.
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

    // Formats active channels into header summary text.
    joystickPortBadge(w) {
        if (!w.multiMotor) {
            return `X: ${w.xPort} | Y: ${w.yPort}`;
        }
        const formatAxis = (motors) => {
            if (!motors || motors.length === 0) return "None";
            return motors.map(m => m.invert ? `-${m.port}` : m.port).join(", ");
        };
        return `X: ${formatAxis(w.xMotors)} | Y: ${formatAxis(w.yMotors)}`;
    }

    // Constructs operational interface markup for an active item.
    renderRunCard(w) {
        const sizeClass = w.type === "joystick" ? "run-card-2x2" : "run-card-2x1";
        const portBadge = w.type === "joystick" ? this.joystickPortBadge(w) : `Port ${w.port}`;
        const cardTitle = TYPE_LABELS[w.type] ?? w.type;
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

    // Wires up interactive behaviors across operational control panels.
    attachRunCardListeners() {
        this.widgets.forEach(w => {
            if (w.type === "joystick") this.initJoystick(w);
            else if (w.type === "slider") this.initSlider(w);
            else if (w.type === "button") this.initButton(w);
        });
    }

    // Maps 2D touch displacement directly to X and Y motor velocities.
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
                const commands = calculateJoystickCommands(w, normX, normY);
                if (commands.length > 0) {
                    this.apiBatch(commands, 400).catch(() => {});
                }
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

            const commands = getJoystickStopCommands(w);
            if (commands.length > 0) {
                this.apiBatch(commands, 400).catch(() => {});
            }
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

    // Connects range bar input events to throttled velocity updates.
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
        const sendCmd = (payload) => this.apiPost(`/api/port/${w.port}`, payload).catch(() => {});

        if (w.action === "momentary") {
            let active = false;
            const start = (e) => {
                e.preventDefault();
                if (active) return;
                active = true;
                btn.classList.add("btn-danger");
                try { btn.setPointerCapture(e.pointerId); } catch (_) {}
                sendCmd({ speed: defaultSpeed });
            };
            const stop = (e) => {
                e.preventDefault();
                if (!active) return;
                active = false;
                btn.classList.remove("btn-danger");
                sendCmd({ command: "stop", stop_action: "brake" });
            };
            btn.addEventListener("pointerdown", start);
            btn.addEventListener("pointerup", stop);
            btn.addEventListener("pointercancel", stop);
            btn.addEventListener("pointerleave", (e) => { if (e.buttons > 0) stop(e); });
        } else if (w.action === "toggle") {
            btn.addEventListener("click", () => {
                const isRunning = btn.classList.toggle("toggle-active");
                btn.classList.toggle("btn-secondary", !isRunning);
                btn.textContent = isRunning ? "Stop Motor" : "Start Motor";
                sendCmd(isRunning ? { speed: defaultSpeed } : { command: "stop", stop_action: "brake" });
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
                const json = await res.json();
                const data = json?.data ?? json;
                const rtt = Math.round(performance.now() - t0);

                this.setConnectionState(true, rtt);
                const volt = data.battery?.voltage_v ?? data.battery_voltage;
                if (this.dom.batteryVal && volt != null) {
                    this.dom.batteryVal.textContent = Number(volt).toFixed(1);
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

    // Refreshes link indicators and round-trip delay readouts.
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

    // Formats rotational status records for the system inspection grid.
    renderDiagnosticsTable(motors) {
        if (!this.dom.telemetryBody) return;
        this.dom.telemetryBody.innerHTML = ["A", "B", "C", "D"].map(port => {
            const m = motors.find(x => x.port === port);
            if (!m || !m.connected) return `<tr><td><strong>${port}</strong></td><td colspan="5" class="text-muted">Disconnected</td></tr>`;
            return `
                <tr>
                    <td><strong>${port}</strong></td>
                    <td><span class="badge ${m.state?.length ? "badge-running" : "badge-ready"}">${m.state?.join(", ") || "Ready"}</span></td>
                    <td>${m.speed}</td>
                    <td>${m.position}</td>
                    <td>${m.duty_cycle}%</td>
                    <td>${m.polarity}</td>
                </tr>
            `;
        }).join("");
    }

    // Updates bottom metadata text across items matching a designated port.
    updateCardFooters(attr, devices, formatText) {
        document.querySelectorAll(`.run-card[${attr}]`).forEach(card => {
            const port = card.getAttribute(attr);
            const dev = devices.find(x => x.port === port);
            const text = (!dev || !dev.connected) ? "Disconnected" : formatText(dev);
            card.querySelectorAll(".run-card-footer").forEach(el => {
                el.textContent = text;
            });
        });
    }

    // Displays numeric metric values and driver metadata on cards.
    updateSensorReadouts(sensors) {
        document.querySelectorAll(".run-card[data-sensor-port]").forEach(card => {
            const port = card.getAttribute("data-sensor-port");
            const s = sensors.find(x => x.port === port);
            const val = (s && s.connected) ? (s.value0 ?? s.value ?? "--") : "--";
            card.querySelectorAll(`.sensor-display-val[data-sensor-port="${port}"]`).forEach(readout => {
                readout.textContent = val;
            });
        });
        this.updateCardFooters("data-sensor-port", sensors, s => `${s.driver_name} (${s.mode})`);
    }

    // Synchronizes action widget footers with hardware link states.
    updateMotorCardStatus(motors) {
        this.updateCardFooters("data-motor-port", motors, m => m.state?.length ? m.state.join(", ") : "Ready");
    }

    // Records timestamped diagnostic entries in the event console.
    log(msg, type = "info") {
        if (!this.dom.logBox) return;
        const entry = document.createElement("div");
        entry.className = `log-entry log-${type}`;
        entry.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
        this.dom.logBox.appendChild(entry);
        this.dom.logBox.scrollTop = this.dom.logBox.scrollHeight;
    }
}

// Initializes UI runtime components when DOM parsing finishes.
window.addEventListener("DOMContentLoaded", () => {
    window.ev3App = new EV3App();
});
