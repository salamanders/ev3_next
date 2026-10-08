/**
 * LEGO Mindstorms EV3 Web Motor Control - Modular Dashboard Application
 * Zero external JavaScript dependencies.
 */

class EV3App {
    constructor() {
        this.activeMode = "design"; // 'design' or 'run'
        this.pollIntervalMs = 250;
        this.isPolling = false;
        this.lastLatency = 0;
        this.consecutiveErrors = 0;
        this.hardwareMotors = [];
        this.hardwareSensors = [];
        this.widgets = [];
        this.throttleMap = new Map();
        this.activeToggles = new Map();

        this.initStorage();
        this.initElements();
        this.attachEventListeners();
        this.startPolling();
        this.rescanHardware();
        this.renderAll();
    }

    // --- Persistence & Defaults ---

    initStorage() {
        const stored = localStorage.getItem("ev3_dashboard_widgets");
        if (stored) {
            try {
                this.widgets = JSON.parse(stored);
            } catch (e) {
                console.error("Failed to parse stored widgets, restoring defaults", e);
                this.loadDefaultWidgets();
            }
        } else {
            this.loadDefaultWidgets();
        }

        const savedMode = localStorage.getItem("ev3_dashboard_mode");
        if (savedMode === "run" || savedMode === "design") {
            this.activeMode = savedMode;
        }
    }

    loadDefaultWidgets() {
        this.widgets = [
            {
                id: "w_" + Date.now() + "_1",
                type: "joystick",
                title: "2D Drive Joystick",
                driveMode: "differential", // 'differential' or 'independent'
                leftPort: "B",
                rightPort: "C",
                xPort: "B",
                yPort: "C",
                invertX: false,
                invertY: false,
                maxSpeed: 800
            },
            {
                id: "w_" + Date.now() + "_2",
                type: "slider",
                title: "Port A Auxiliary Slider",
                port: "A",
                maxSpeed: 1050
            },
            {
                id: "w_" + Date.now() + "_3",
                type: "step",
                title: "Port D Turn +90°",
                port: "D",
                speed: 600,
                degrees: 90
            }
        ];
        this.saveWidgets();
    }

    saveWidgets() {
        localStorage.setItem("ev3_dashboard_widgets", JSON.stringify(this.widgets));
    }

    // --- DOM Elements ---

    initElements() {
        // Badges
        this.connBadge = document.getElementById("conn-badge");
        this.connText = document.getElementById("conn-text");
        this.latencyVal = document.getElementById("latency-val");
        this.batteryVal = document.getElementById("battery-val");

        // Header controls
        this.btnEstopHeader = document.getElementById("btn-estop-header");
        this.btnShutdownHeader = document.getElementById("btn-shutdown-header");

        // Mode switchers
        this.btnModeDesign = document.getElementById("btn-mode-design");
        this.btnModeRun = document.getElementById("btn-mode-run");
        this.btnRescan = document.getElementById("btn-rescan");

        // Views
        this.designView = document.getElementById("design-view");
        this.runView = document.getElementById("run-view");

        // Design elements
        this.hardwareList = document.getElementById("hardware-list");
        this.hwCountBadge = document.getElementById("hw-count-badge");
        this.widgetTypeSelect = document.getElementById("widget-type-select");
        this.btnAddWidget = document.getElementById("btn-add-widget");
        this.btnResetLayout = document.getElementById("btn-reset-layout");
        this.designWidgetList = document.getElementById("design-widget-list");
        this.widgetCountLabel = document.getElementById("widget-count-label");

        // Run elements
        this.runWidgetList = document.getElementById("run-widget-list");
        this.runEmptyNotice = document.getElementById("run-empty-notice");
        this.btnEmptyGotoDesign = document.getElementById("btn-empty-goto-design");

        // Diagnostics
        this.telemetryTableBody = document.getElementById("telemetry-table-body");
        this.logBox = document.getElementById("log-box");
        this.btnClearLog = document.getElementById("btn-clear-log");
    }

    attachEventListeners() {
        // Mode switching
        this.btnModeDesign.addEventListener("click", () => this.setMode("design"));
        this.btnModeRun.addEventListener("click", () => this.setMode("run"));
        if (this.btnEmptyGotoDesign) {
            this.btnEmptyGotoDesign.addEventListener("click", () => this.setMode("design"));
        }

        // Hardware rescan
        this.btnRescan.addEventListener("click", () => this.rescanHardware());

        // Widget builder actions
        this.btnAddWidget.addEventListener("click", () => {
            const type = this.widgetTypeSelect.value;
            this.addWidget(type);
        });

        this.btnResetLayout.addEventListener("click", () => {
            if (confirm("Reset dashboard to default widgets? Your customized layout will be overwritten.")) {
                this.loadDefaultWidgets();
                this.renderAll();
                this.log("Dashboard widgets reset to defaults.", "info");
            }
        });

        // Emergency Stop
        this.btnEstopHeader.addEventListener("click", () => this.emergencyStop());
        window.addEventListener("keydown", (e) => {
            if (e.code === "Space" && e.target.tagName !== "INPUT") {
                e.preventDefault();
                this.emergencyStop();
            }
        });

        // Safe Shutdown
        if (this.btnShutdownHeader) {
            this.btnShutdownHeader.addEventListener("click", async () => {
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

        // Clear Log
        if (this.btnClearLog) {
            this.btnClearLog.addEventListener("click", () => {
                this.logBox.innerHTML = "";
            });
        }
    }

    // --- Mode Management ---

    setMode(mode) {
        this.activeMode = mode;
        localStorage.setItem("ev3_dashboard_mode", mode);

        if (mode === "design") {
            this.btnModeDesign.classList.add("active");
            this.btnModeRun.classList.remove("active");
            this.designView.style.display = "flex";
            this.runView.style.display = "none";
            this.renderDesignWidgets();
        } else {
            this.btnModeDesign.classList.remove("active");
            this.btnModeRun.classList.add("active");
            this.designView.style.display = "none";
            this.runView.style.display = "flex";
            this.renderRunWidgets();
        }
    }

    // --- Hardware Rescan ---

    async rescanHardware() {
        this.btnRescan.disabled = true;
        this.btnRescan.textContent = "⏳ Scanning...";
        this.log("Scanning hardware for connected motors and sensors...", "info");

        try {
            await this.apiPost("/api/rescan", {});
            const res = await fetch("/api/ports", { cache: "no-store" });
            if (res.ok) {
                const data = await res.json();
                if (data.success && data.data) {
                    this.hardwareMotors = data.data.motors || [];
                    this.hardwareSensors = data.data.sensors || [];
                    this.renderHardwarePanel();
                    this.updatePortSelectOptions();
                    const mCount = this.hardwareMotors.filter(m => m.connected).length;
                    const sCount = this.hardwareSensors.filter(s => s.connected).length;
                    this.log(`Rescan complete: found ${mCount} motor(s) and ${sCount} sensor(s).`, "success");
                }
            }
        } catch (err) {
            this.log("Hardware rescan error: " + err.message, "error");
        } finally {
            this.btnRescan.disabled = false;
            this.btnRescan.textContent = "🔄 Rescan Hardware";
        }
    }

    renderHardwarePanel() {
        const motorPorts = ["A", "B", "C", "D"];
        const sensorPorts = ["1", "2", "3", "4"];
        let connectedCount = 0;
        let html = "";

        motorPorts.forEach(port => {
            const motor = (this.hardwareMotors || []).find(m => m.port === port);
            const isConn = motor && motor.connected;
            if (isConn) connectedCount++;

            html += `
                <div class="hw-item ${isConn ? 'hw-connected' : 'hw-disconnected'}">
                    <div>
                        <span class="hw-port-badge">Port ${port}</span>
                        <div class="hw-driver">${isConn ? motor.driver_name : 'No motor detected'}</div>
                    </div>
                    <span class="badge ${isConn ? 'badge-connected' : 'badge-info'}">
                        ${isConn ? 'Connected' : 'Empty'}
                    </span>
                </div>
            `;
        });

        sensorPorts.forEach(port => {
            const sensor = (this.hardwareSensors || []).find(s => s.port === port);
            const isConn = sensor && sensor.connected;
            if (isConn) connectedCount++;

            html += `
                <div class="hw-item ${isConn ? 'hw-connected' : 'hw-disconnected'}">
                    <div>
                        <span class="hw-port-badge">Port ${port}</span>
                        <div class="hw-driver">${isConn ? sensor.driver_name : 'No sensor detected'}</div>
                    </div>
                    <span class="badge ${isConn ? 'badge-connected' : 'badge-info'}">
                        ${isConn ? (sensor.mode || 'Connected') : 'Empty'}
                    </span>
                </div>
            `;
        });

        this.hardwareList.innerHTML = html;
        this.hwCountBadge.textContent = `${connectedCount} / 8 Connected`;
    }

    updatePortSelectOptions() {
        // Refresh all port select dropdowns in design mode cards
        const selects = document.querySelectorAll(".port-select");
        selects.forEach(select => {
            const currentVal = select.value;
            const allowNone = select.dataset.allowNone === "true";
            let opts = "";
            if (allowNone) {
                opts += `<option value="none" ${currentVal === "none" ? "selected" : ""}>None</option>`;
            }
            ["A", "B", "C", "D"].forEach(p => {
                const motor = this.hardwareMotors.find(m => m.port === p);
                const isConn = motor && motor.connected;
                const label = isConn ? `Port ${p} (${motor.driver_name})` : `Port ${p}`;
                opts += `<option value="${p}" ${currentVal === p ? "selected" : ""}>${label}</option>`;
            });
            select.innerHTML = opts;
        });
    }

    // --- Widget Model Management ---

    addWidget(type) {
        const id = "w_" + Date.now();
        let newWidget = { id, type };

        switch (type) {
            case "joystick":
                newWidget.title = "2D Joystick";
                newWidget.driveMode = "differential";
                newWidget.leftPort = "B";
                newWidget.rightPort = "C";
                newWidget.xPort = "B";
                newWidget.yPort = "C";
                newWidget.invertX = false;
                newWidget.invertY = false;
                newWidget.maxSpeed = 800;
                break;
            case "slider":
                newWidget.title = "Speed Slider";
                newWidget.port = "A";
                newWidget.maxSpeed = 1050;
                break;
            case "momentary":
                newWidget.title = "Press-to-Run Button";
                newWidget.port = "A";
                newWidget.speed = 600;
                break;
            case "toggle":
                newWidget.title = "Run/Stop Toggle";
                newWidget.port = "A";
                newWidget.speed = 600;
                break;
            case "timed":
                newWidget.title = "Timed Move Button";
                newWidget.port = "A";
                newWidget.speed = 600;
                newWidget.seconds = 2.0;
                break;
            case "step":
                newWidget.title = "Step Angle Button";
                newWidget.port = "A";
                newWidget.speed = 600;
                newWidget.degrees = 90;
                break;
            case "sensor":
                newWidget.title = "Sensor Reading";
                newWidget.port = "1";
                newWidget.mode = "TOUCH";
                break;
        }

        this.widgets.push(newWidget);
        this.saveWidgets();
        this.renderDesignWidgets();
        this.log(`Added widget: ${newWidget.title} (${type})`, "info");
    }

    removeWidget(id) {
        this.widgets = this.widgets.filter(w => w.id !== id);
        this.saveWidgets();
        this.renderDesignWidgets();
        this.log("Widget removed.", "info");
    }

    updateWidget(id, updates) {
        const w = this.widgets.find(w => w.id === id);
        if (w) {
            Object.assign(w, updates);
            this.saveWidgets();
        }
    }

    // --- Rendering ---

    renderAll() {
        this.renderHardwarePanel();
        if (this.activeMode === "design") {
            this.setMode("design");
        } else {
            this.setMode("run");
        }
    }

    renderDesignWidgets() {
        this.widgetCountLabel.textContent = `${this.widgets.length} widget${this.widgets.length === 1 ? '' : 's'}`;

        if (this.widgets.length === 0) {
            this.designWidgetList.innerHTML = `
                <div class="card empty-notice" style="grid-column: 1 / -1;">
                    <p>No widgets added yet. Select a widget type above and click <strong>➕ Add Widget</strong>.</p>
                </div>
            `;
            return;
        }

        let html = "";
        this.widgets.forEach(w => {
            html += this.renderDesignCard(w);
        });
        this.designWidgetList.innerHTML = html;
        this.attachDesignCardListeners();
    }

    renderDesignCard(w) {
        const portOptions = (selectedPort, allowNone = false) => {
            let opts = allowNone ? `<option value="none" ${selectedPort === 'none' ? 'selected' : ''}>None</option>` : '';
            ["A", "B", "C", "D"].forEach(p => {
                const motor = this.hardwareMotors.find(m => m.port === p);
                const isConn = motor && motor.connected;
                const label = isConn ? `Port ${p} (${motor.driver_name})` : `Port ${p}`;
                opts += `<option value="${p}" ${selectedPort === p ? 'selected' : ''}>${label}</option>`;
            });
            return opts;
        };

        let fieldsHtml = "";

        if (w.type === "joystick") {
            fieldsHtml = `
                <div class="widget-config-field full-width">
                    <label>Drive Mode</label>
                    <select class="field-drive-mode" data-id="${w.id}">
                        <option value="differential" ${w.driveMode === 'differential' ? 'selected' : ''}>Differential / Tank Drive (Left & Right Motors)</option>
                        <option value="independent" ${w.driveMode === 'independent' ? 'selected' : ''}>Independent Axes (X Axis & Y Axis Motors)</option>
                    </select>
                </div>
                ${w.driveMode === 'differential' ? `
                    <div class="widget-config-field">
                        <label>Left Motor</label>
                        <select class="port-select field-left-port" data-id="${w.id}">${portOptions(w.leftPort)}</select>
                    </div>
                    <div class="widget-config-field">
                        <label>Right Motor</label>
                        <select class="port-select field-right-port" data-id="${w.id}">${portOptions(w.rightPort)}</select>
                    </div>
                ` : `
                    <div class="widget-config-field">
                        <label>X Axis Motor</label>
                        <select class="port-select field-x-port" data-id="${w.id}" data-allow-none="true">${portOptions(w.xPort, true)}</select>
                    </div>
                    <div class="widget-config-field">
                        <label>Y Axis Motor</label>
                        <select class="port-select field-y-port" data-id="${w.id}" data-allow-none="true">${portOptions(w.yPort, true)}</select>
                    </div>
                `}
                <div class="widget-config-field">
                    <label>Invert X Axis</label>
                    <label style="text-transform:none; font-weight:normal; display:flex; align-items:center; gap:6px;">
                        <input type="checkbox" class="field-invert-x" data-id="${w.id}" ${w.invertX ? 'checked' : ''}> Invert X
                    </label>
                </div>
                <div class="widget-config-field">
                    <label>Invert Y Axis</label>
                    <label style="text-transform:none; font-weight:normal; display:flex; align-items:center; gap:6px;">
                        <input type="checkbox" class="field-invert-y" data-id="${w.id}" ${w.invertY ? 'checked' : ''}> Invert Y
                    </label>
                </div>
                <div class="widget-config-field full-width">
                    <label>Max Speed (ticks/s)</label>
                    <input type="number" class="field-max-speed" data-id="${w.id}" min="100" max="1560" step="50" value="${w.maxSpeed}">
                </div>
            `;
        } else if (w.type === "slider") {
            fieldsHtml = `
                <div class="widget-config-field">
                    <label>Target Motor</label>
                    <select class="port-select field-port" data-id="${w.id}">${portOptions(w.port)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Max Speed (ticks/s)</label>
                    <input type="number" class="field-max-speed" data-id="${w.id}" min="100" max="1560" step="50" value="${w.maxSpeed}">
                </div>
            `;
        } else if (w.type === "momentary" || w.type === "toggle") {
            fieldsHtml = `
                <div class="widget-config-field">
                    <label>Target Motor</label>
                    <select class="port-select field-port" data-id="${w.id}">${portOptions(w.port)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Speed (ticks/s)</label>
                    <input type="number" class="field-speed" data-id="${w.id}" min="-1560" max="1560" step="50" value="${w.speed}">
                </div>
            `;
        } else if (w.type === "timed") {
            fieldsHtml = `
                <div class="widget-config-field">
                    <label>Target Motor</label>
                    <select class="port-select field-port" data-id="${w.id}">${portOptions(w.port)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Speed (ticks/s)</label>
                    <input type="number" class="field-speed" data-id="${w.id}" min="-1560" max="1560" step="50" value="${w.speed}">
                </div>
                <div class="widget-config-field full-width">
                    <label>Duration (Seconds)</label>
                    <input type="number" class="field-seconds" data-id="${w.id}" min="0.1" max="60" step="0.1" value="${w.seconds}">
                </div>
            `;
        } else if (w.type === "step") {
            fieldsHtml = `
                <div class="widget-config-field">
                    <label>Target Motor</label>
                    <select class="port-select field-port" data-id="${w.id}">${portOptions(w.port)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Speed (ticks/s)</label>
                    <input type="number" class="field-speed" data-id="${w.id}" min="50" max="1560" step="50" value="${w.speed}">
                </div>
                <div class="widget-config-field full-width">
                    <label>Angle (Degrees)</label>
                    <input type="number" class="field-degrees" data-id="${w.id}" min="-3600" max="3600" step="45" value="${w.degrees}">
                </div>
            `;
        } else if (w.type === "sensor") {
            const sensorPortOptions = (selectedPort) => {
                let opts = "";
                ["1", "2", "3", "4"].forEach(p => {
                    const sensor = (this.hardwareSensors || []).find(s => s.port === p);
                    const isConn = sensor && sensor.connected;
                    const label = isConn ? `Port ${p} (${sensor.driver_name})` : `Port ${p}`;
                    opts += `<option value="${p}" ${selectedPort === p ? 'selected' : ''}>${label}</option>`;
                });
                return opts;
            };

            const selectedSensor = (this.hardwareSensors || []).find(s => s.port === (w.port || "1"));
            const modes = (selectedSensor && selectedSensor.modes && selectedSensor.modes.length > 0)
                ? selectedSensor.modes
                : ["TOUCH", "COL-COLOR", "COL-REFLECT", "US-DIST-CM", "GYRO-ANG"];
            let modeOpts = "";
            modes.forEach(m => {
                modeOpts += `<option value="${m}" ${w.mode === m ? 'selected' : ''}>${m}</option>`;
            });

            fieldsHtml = `
                <div class="widget-config-field">
                    <label>Input Port</label>
                    <select class="field-sensor-port" data-id="${w.id}">${sensorPortOptions(w.port || "1")}</select>
                </div>
                <div class="widget-config-field">
                    <label>Sensor Mode</label>
                    <select class="field-sensor-mode" data-id="${w.id}">${modeOpts}</select>
                </div>
            `;
        }

        const typeLabels = {
            joystick: "🎮 2D Virtual Joystick",
            slider: "🎚️ Speed Slider",
            momentary: "🔘 Momentary Button",
            toggle: "🔁 Toggle Button",
            timed: "⏱️ Timed Move",
            step: "🔄 Step Angle",
            sensor: "👁️ Sensor Display"
        };

        return `
            <div class="design-card" id="card-${w.id}">
                <div class="design-card-header">
                    <div class="design-card-title">
                        <span>${typeLabels[w.type] || w.type}</span>
                    </div>
                    <button class="btn btn-sm btn-danger btn-remove-widget" data-id="${w.id}">
                        🗑️ Remove
                    </button>
                </div>

                <div class="widget-config-field full-width">
                    <label>Widget Title</label>
                    <input type="text" class="field-title" data-id="${w.id}" value="${w.title}">
                </div>

                <div class="widget-config-grid">
                    ${fieldsHtml}
                </div>
            </div>
        `;
    }

    attachDesignCardListeners() {
        // Remove button
        document.querySelectorAll(".btn-remove-widget").forEach(btn => {
            btn.addEventListener("click", () => this.removeWidget(btn.dataset.id));
        });

        // Title change
        document.querySelectorAll(".field-title").forEach(inp => {
            inp.addEventListener("change", (e) => this.updateWidget(e.target.dataset.id, { title: e.target.value }));
        });

        // Joystick drive mode change
        document.querySelectorAll(".field-drive-mode").forEach(sel => {
            sel.addEventListener("change", (e) => {
                this.updateWidget(e.target.dataset.id, { driveMode: e.target.value });
                this.renderDesignWidgets();
            });
        });

        // Sensor input port change
        document.querySelectorAll(".field-sensor-port").forEach(sel => {
            sel.addEventListener("change", (e) => {
                const port = e.target.value;
                const widget = this.widgets.find(w => w.id === e.target.dataset.id);
                if (widget) {
                    widget.port = port;
                    const sensor = (this.hardwareSensors || []).find(s => s.port === port);
                    if (sensor && sensor.mode) {
                        widget.mode = sensor.mode;
                    }
                    this.saveWidgets();
                    this.renderDesignWidgets();
                }
            });
        });

        // Sensor mode change
        document.querySelectorAll(".field-sensor-mode").forEach(sel => {
            sel.addEventListener("change", async (e) => {
                const mode = e.target.value;
                const widget = this.widgets.find(w => w.id === e.target.dataset.id);
                if (widget) {
                    widget.mode = mode;
                    this.saveWidgets();
                    try {
                        await this.apiPort(widget.port, { mode });
                        this.log(`Sensor port ${widget.port} mode changed to ${mode}`, "info");
                    } catch (err) {
                        this.log(`Mode change failed: ${err.message}`, "error");
                    }
                }
            });
        });

        // Ports and other fields
        const bindField = (selector, key, parser = (v) => v) => {
            document.querySelectorAll(selector).forEach(elem => {
                elem.addEventListener("change", (e) => {
                    const val = elem.type === "checkbox" ? elem.checked : parser(e.target.value);
                    this.updateWidget(e.target.dataset.id, { [key]: val });
                });
            });
        };

        bindField(".field-left-port", "leftPort");
        bindField(".field-right-port", "rightPort");
        bindField(".field-x-port", "xPort");
        bindField(".field-y-port", "yPort");
        bindField(".field-port", "port");
        bindField(".field-invert-x", "invertX");
        bindField(".field-invert-y", "invertY");
        bindField(".field-max-speed", "maxSpeed", v => parseInt(v, 10));
        bindField(".field-speed", "speed", v => parseInt(v, 10));
        bindField(".field-seconds", "seconds", v => parseFloat(v));
        bindField(".field-degrees", "degrees", v => parseInt(v, 10));
    }

    renderRunWidgets() {
        if (this.widgets.length === 0) {
            this.runEmptyNotice.style.display = "block";
            this.runWidgetList.innerHTML = "";
            return;
        }

        this.runEmptyNotice.style.display = "none";
        let html = "";
        this.widgets.forEach(w => {
            html += this.renderRunCard(w);
        });
        this.runWidgetList.innerHTML = html;
        this.attachRunCardListeners();
    }

    renderRunCard(w) {
        let contentHtml = "";

        if (w.type === "joystick") {
            contentHtml = `
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
            contentHtml = `
                <div class="run-slider-box">
                    <div class="slider-val-row">
                        <span>Speed: <strong class="slider-val-readout" id="slider-val-${w.id}">0</strong> ticks/s</span>
                        <span class="text-muted">(Max: ${w.maxSpeed})</span>
                    </div>
                    <input type="range" class="run-range-slider" id="slider-input-${w.id}"
                           min="-${w.maxSpeed}" max="${w.maxSpeed}" value="0" step="25" data-id="${w.id}">
                    <div class="slider-actions">
                        <button class="btn btn-sm btn-secondary btn-slider-zero" data-id="${w.id}">Zero (Stop)</button>
                    </div>
                </div>
            `;
        } else if (w.type === "momentary") {
            contentHtml = `
                <div class="run-button-box">
                    <button class="btn btn-primary btn-big-action btn-momentary" id="btn-mom-${w.id}" data-id="${w.id}">
                        Press &amp; Hold to Run (${w.speed > 0 ? '+' : ''}${w.speed})
                    </button>
                </div>
            `;
        } else if (w.type === "toggle") {
            const isActive = this.activeToggles.get(w.id) || false;
            contentHtml = `
                <div class="run-button-box">
                    <button class="btn btn-big-action btn-toggle ${isActive ? 'toggle-active' : 'btn-secondary'}" id="btn-tog-${w.id}" data-id="${w.id}">
                        ${isActive ? '⏹ Stop Motor' : `▶ Start Motor (${w.speed > 0 ? '+' : ''}${w.speed})`}
                    </button>
                </div>
            `;
        } else if (w.type === "timed") {
            contentHtml = `
                <div class="run-button-box">
                    <button class="btn btn-primary btn-big-action btn-timed" id="btn-timed-${w.id}" data-id="${w.id}">
                        ⏱️ Run for ${w.seconds}s (${w.speed} ticks/s)
                    </button>
                </div>
            `;
        } else if (w.type === "step") {
            contentHtml = `
                <div class="run-button-box">
                    <button class="btn btn-primary btn-big-action btn-step" id="btn-step-${w.id}" data-id="${w.id}">
                        🔄 Turn ${w.degrees > 0 ? '+' : ''}${w.degrees}° (${w.speed} ticks/s)
                    </button>
                </div>
            `;
        } else if (w.type === "sensor") {
            const sensor = (this.hardwareSensors || []).find(s => s.port === w.port);
            const val = (sensor && sensor.connected) ? sensor.value0 : "--";
            const units = (sensor && sensor.connected) ? sensor.units : "";
            const mode = (sensor && sensor.connected) ? sensor.mode : (w.mode || "--");
            contentHtml = `
                <div class="run-sensor-box" style="text-align: center; padding: 18px 0;">
                    <div style="font-size: 2.2rem; font-weight: 700; color: #2563eb;" id="sensor-val-${w.id}">
                        <span id="sensor-num-${w.id}">${val}</span> <span style="font-size: 1.1rem; color: #64748b;" id="sensor-unit-${w.id}">${units}</span>
                    </div>
                    <div style="font-size: 0.95rem; color: #475569; margin-top: 8px;">
                        Mode: <strong id="sensor-mode-${w.id}">${mode}</strong>
                    </div>
                </div>
            `;
        }

        let portBadge = "";
        if (w.type === "joystick") {
            portBadge = w.driveMode === "differential" ? `Ports ${w.leftPort}+${w.rightPort}` : `X:${w.xPort} Y:${w.yPort}`;
        } else {
            portBadge = `Port ${w.port}`;
        }

        return `
            <div class="run-card" id="run-card-${w.id}">
                <div class="run-card-header">
                    <span class="run-card-title">${w.title}</span>
                    <span class="badge badge-info">${portBadge}</span>
                </div>
                ${contentHtml}
                <div class="run-card-status" id="run-status-${w.id}">
                    Status: Ready
                </div>
            </div>
        `;
    }

    attachRunCardListeners() {
        this.widgets.forEach(w => {
            if (w.type === "joystick") {
                this.initJoystickWidget(w);
            } else if (w.type === "slider") {
                this.initSliderWidget(w);
            } else if (w.type === "momentary") {
                this.initMomentaryWidget(w);
            } else if (w.type === "toggle") {
                this.initToggleWidget(w);
            } else if (w.type === "timed") {
                this.initTimedWidget(w);
            } else if (w.type === "step") {
                this.initStepWidget(w);
            }
        });
    }

    // --- Interactive Widget Handlers ---

    // 15 Hz Throttler to protect 300 MHz CPU
    throttle(key, intervalMs, fn) {
        const now = performance.now();
        const entry = this.throttleMap.get(key) || { lastTime: 0, timer: null };

        if (now - entry.lastTime >= intervalMs) {
            entry.lastTime = now;
            if (entry.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }
            this.throttleMap.set(key, entry);
            fn();
        } else if (!entry.timer) {
            const delay = intervalMs - (now - entry.lastTime);
            entry.timer = setTimeout(() => {
                entry.lastTime = performance.now();
                entry.timer = null;
                this.throttleMap.set(key, entry);
                fn();
            }, delay);
            this.throttleMap.set(key, entry);
        }
    }

    initJoystickWidget(w) {
        const boundary = document.getElementById(`joy-boundary-${w.id}`);
        const knob = document.getElementById(`joy-knob-${w.id}`);
        const readout = document.getElementById(`joy-readout-${w.id}`);
        if (!boundary || !knob) return;

        let dragging = false;
        const maxRadius = 70; // 200px boundary - 56px knob / 2 ≈ 72px

        const updatePosition = (clientX, clientY) => {
            const rect = boundary.getBoundingClientRect();
            const centerX = rect.left + rect.width / 2;
            const centerY = rect.top + rect.height / 2;

            let dx = clientX - centerX;
            let dy = clientY - centerY;
            const dist = Math.hypot(dx, dy);

            if (dist > maxRadius) {
                dx = (dx / dist) * maxRadius;
                dy = (dy / dist) * maxRadius;
            }

            knob.style.transform = `translate(${dx}px, ${dy}px)`;

            let normX = dx / maxRadius;
            let normY = -dy / maxRadius; // Up is positive

            if (w.invertX) normX = -normX;
            if (w.invertY) normY = -normY;

            readout.textContent = `X: ${Math.round(normX * 100)}% | Y: ${Math.round(normY * 100)}%`;

            // 15 Hz Dispatch (~66 ms)
            this.throttle(`joy_${w.id}`, 66, () => {
                if (w.driveMode === "differential") {
                    const leftSpeed = Math.max(-w.maxSpeed, Math.min(w.maxSpeed, Math.round((normY + normX) * w.maxSpeed)));
                    const rightSpeed = Math.max(-w.maxSpeed, Math.min(w.maxSpeed, Math.round((normY - normX) * w.maxSpeed)));
                    this.apiPost("/api/tank-drive", {
                        left_port: w.leftPort,
                        right_port: w.rightPort,
                        left_speed: leftSpeed,
                        right_speed: rightSpeed
                    }).catch(() => {});
                } else {
                    if (w.xPort && w.xPort !== "none") {
                        const speedX = Math.round(normX * w.maxSpeed);
                        this.apiPost(`/api/motor/${w.xPort}/run-forever`, { speed: speedX }).catch(() => {});
                    }
                    if (w.yPort && w.yPort !== "none") {
                        const speedY = Math.round(normY * w.maxSpeed);
                        this.apiPost(`/api/motor/${w.yPort}/run-forever`, { speed: speedY }).catch(() => {});
                    }
                }
            });
        };

        const stopJoystick = () => {
            if (!dragging) return;
            dragging = false;
            knob.classList.remove("active");
            knob.style.transform = "translate(0px, 0px)";
            readout.textContent = "X: 0% | Y: 0%";

            // Clear throttle timer and send immediate halt
            const entry = this.throttleMap.get(`joy_${w.id}`);
            if (entry && entry.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }

            if (w.driveMode === "differential") {
                this.apiPost("/api/tank-drive", {
                    left_port: w.leftPort,
                    right_port: w.rightPort,
                    left_speed: 0,
                    right_speed: 0
                }).catch(() => {});
            } else {
                if (w.xPort && w.xPort !== "none") {
                    this.apiPost(`/api/motor/${w.xPort}/stop`, { action: "brake" }).catch(() => {});
                }
                if (w.yPort && w.yPort !== "none") {
                    this.apiPost(`/api/motor/${w.yPort}/stop`, { action: "brake" }).catch(() => {});
                }
            }
        };

        boundary.addEventListener("pointerdown", (e) => {
            dragging = true;
            knob.classList.add("active");
            boundary.setPointerCapture(e.pointerId);
            updatePosition(e.clientX, e.clientY);
        });

        boundary.addEventListener("pointermove", (e) => {
            if (dragging) updatePosition(e.clientX, e.clientY);
        });

        boundary.addEventListener("pointerup", (e) => {
            boundary.releasePointerCapture(e.pointerId);
            stopJoystick();
        });

        boundary.addEventListener("pointercancel", () => stopJoystick());
    }

    initSliderWidget(w) {
        const input = document.getElementById(`slider-input-${w.id}`);
        const valReadout = document.getElementById(`slider-val-${w.id}`);
        const zeroBtn = document.querySelector(`.btn-slider-zero[data-id="${w.id}"]`);
        if (!input) return;

        input.addEventListener("input", (e) => {
            const speed = parseInt(e.target.value, 10);
            valReadout.textContent = speed;

            this.throttle(`slider_${w.id}`, 66, () => {
                if (speed === 0) {
                    this.apiPost(`/api/motor/${w.port}/stop`, { action: "brake" }).catch(() => {});
                } else {
                    this.apiPost(`/api/motor/${w.port}/run-forever`, { speed }).catch(() => {});
                }
            });
        });

        if (zeroBtn) {
            zeroBtn.addEventListener("click", () => {
                input.value = 0;
                valReadout.textContent = 0;
                this.apiPost(`/api/motor/${w.port}/stop`, { action: "brake" }).catch(() => {});
            });
        }
    }

    initMomentaryWidget(w) {
        const btn = document.getElementById(`btn-mom-${w.id}`);
        if (!btn) return;

        let active = false;

        const start = (e) => {
            e.preventDefault();
            if (active) return;
            active = true;
            btn.classList.add("btn-danger");
            this.apiPost(`/api/motor/${w.port}/run-forever`, { speed: w.speed }).catch(() => {});
        };

        const stop = (e) => {
            e.preventDefault();
            if (!active) return;
            active = false;
            btn.classList.remove("btn-danger");
            this.apiPost(`/api/motor/${w.port}/stop`, { action: "brake" }).catch(() => {});
        };

        btn.addEventListener("pointerdown", start);
        btn.addEventListener("pointerup", stop);
        btn.addEventListener("pointercancel", stop);
        btn.addEventListener("pointerleave", (e) => {
            if (e.buttons > 0) stop(e);
        });
    }

    initToggleWidget(w) {
        const btn = document.getElementById(`btn-tog-${w.id}`);
        if (!btn) return;

        btn.addEventListener("click", () => {
            const current = this.activeToggles.get(w.id) || false;
            const next = !current;
            this.activeToggles.set(w.id, next);

            if (next) {
                btn.className = "btn btn-big-action btn-toggle toggle-active";
                btn.textContent = "⏹ Stop Motor";
                this.apiPost(`/api/motor/${w.port}/run-forever`, { speed: w.speed }).catch(() => {});
            } else {
                btn.className = "btn btn-big-action btn-toggle btn-secondary";
                btn.textContent = `▶ Start Motor (${w.speed > 0 ? '+' : ''}${w.speed})`;
                this.apiPost(`/api/motor/${w.port}/stop`, { action: "brake" }).catch(() => {});
            }
        });
    }

    initTimedWidget(w) {
        const btn = document.getElementById(`btn-timed-${w.id}`);
        if (!btn) return;

        btn.addEventListener("click", async () => {
            btn.disabled = true;
            const origText = btn.textContent;
            btn.textContent = "⏳ Running...";

            try {
                const time_ms = Math.round(w.seconds * 1000);
                await this.apiPost(`/api/motor/${w.port}/run-timed`, {
                    speed: w.speed,
                    time_ms,
                    stop_action: "brake"
                });
                setTimeout(() => {
                    btn.disabled = false;
                    btn.textContent = origText;
                }, time_ms);
            } catch (err) {
                btn.disabled = false;
                btn.textContent = origText;
            }
        });
    }

    initStepWidget(w) {
        const btn = document.getElementById(`btn-step-${w.id}`);
        if (!btn) return;

        btn.addEventListener("click", async () => {
            btn.disabled = true;
            const origText = btn.textContent;
            btn.textContent = "⏳ Rotating...";

            try {
                await this.apiPost(`/api/motor/${w.port}/run-to-rel-pos`, {
                    speed: w.speed,
                    position_sp: w.degrees,
                    stop_action: "hold"
                });
                setTimeout(() => {
                    btn.disabled = false;
                    btn.textContent = origText;
                }, 800);
            } catch (err) {
                btn.disabled = false;
                btn.textContent = origText;
            }
        });
    }

    // --- Global Emergency Stop ---

    async emergencyStop() {
        this.log("⚠️ EMERGENCY STOP TRIGGERED", "error");

        // Clear all active toggle states
        this.activeToggles.clear();
        this.throttleMap.clear();

        // Reset UI widgets
        document.querySelectorAll(".run-range-slider").forEach(sl => {
            sl.value = 0;
            const v = document.getElementById(`slider-val-${sl.dataset.id}`);
            if (v) v.textContent = 0;
        });

        document.querySelectorAll(".btn-toggle").forEach(btn => {
            btn.className = "btn btn-big-action btn-toggle btn-secondary";
            const w = this.widgets.find(item => item.id === btn.dataset.id);
            if (w) btn.textContent = `▶ Start Motor (${w.speed > 0 ? '+' : ''}${w.speed})`;
        });

        try {
            const data = await this.apiPost("/api/emergency-stop", {});
            if (data.success) {
                this.log("All motors halted successfully.", "success");
            }
        } catch (e) {
            this.log("Emergency stop request failed: " + e.message, "error");
        }
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
                    const sensors = (data.data && Array.isArray(data.data.sensors)) ? data.data.sensors : [];
                    if (Array.isArray(motors)) {
                        this.hardwareMotors = motors;
                        this.hardwareSensors = sensors;
                        this.updateDiagnostics(motors, sensors);
                        this.updateRunCardStatus(motors, sensors);
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

    updateDiagnostics(motors, sensors = []) {
        if (!this.telemetryTableBody) return;
        let html = "";
        motors.forEach(m => {
            html += `
                <tr>
                    <td><strong>Port ${m.port} (Motor)</strong></td>
                    <td><span class="badge ${m.connected ? 'badge-connected' : 'badge-info'}">${m.connected ? 'Online' : 'Empty'}</span></td>
                    <td>${m.speed} ticks/s</td>
                    <td>${m.position}°</td>
                    <td>${m.duty_cycle}%</td>
                    <td>${m.polarity}</td>
                </tr>
            `;
        });
        sensors.forEach(s => {
            html += `
                <tr>
                    <td><strong>Port ${s.port} (Sensor)</strong></td>
                    <td><span class="badge ${s.connected ? 'badge-connected' : 'badge-info'}">${s.connected ? s.mode : 'Empty'}</span></td>
                    <td>${s.value0} ${s.units}</td>
                    <td>--</td>
                    <td>--</td>
                    <td>--</td>
                </tr>
            `;
        });
        this.telemetryTableBody.innerHTML = html;
    }

    updateRunCardStatus(motors, sensors = []) {
        this.widgets.forEach(w => {
            const statusElem = document.getElementById(`run-status-${w.id}`);
            if (!statusElem) return;

            if (w.type === "sensor") {
                const s = sensors.find(item => item.port === w.port);
                if (s) {
                    const numElem = document.getElementById(`sensor-num-${w.id}`);
                    const unitElem = document.getElementById(`sensor-unit-${w.id}`);
                    const modeElem = document.getElementById(`sensor-mode-${w.id}`);
                    if (numElem) numElem.textContent = s.connected ? s.value0 : "--";
                    if (unitElem) unitElem.textContent = s.connected ? s.units : "";
                    if (modeElem) modeElem.textContent = s.connected ? s.mode : "--";
                    statusElem.textContent = s.connected
                        ? `Mode: ${s.mode} | Driver: ${s.driver_name}`
                        : `Sensor disconnected`;
                }
            } else if (w.type === "joystick") {
                if (w.driveMode === "differential") {
                    const mLeft = motors.find(m => m.port === w.leftPort);
                    const mRight = motors.find(m => m.port === w.rightPort);
                    statusElem.textContent = `L(${w.leftPort}): ${mLeft ? mLeft.speed : 0} t/s | R(${w.rightPort}): ${mRight ? mRight.speed : 0} t/s`;
                } else {
                    const mX = motors.find(m => m.port === w.xPort);
                    const mY = motors.find(m => m.port === w.yPort);
                    statusElem.textContent = `X(${w.xPort}): ${mX ? mX.speed : 0} t/s | Y(${w.yPort}): ${mY ? mY.speed : 0} t/s`;
                }
            } else if (w.port) {
                const m = motors.find(item => item.port === w.port);
                if (m) {
                    statusElem.textContent = `Speed: ${m.speed} t/s | Angle: ${m.position}° | Duty: ${m.duty_cycle}%`;
                }
            }
        });
    }

    // --- API & Logging ---

    async apiPost(endpoint, payload) {
        const res = await fetch(endpoint, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload)
        });
        if (!res.ok) {
            throw new Error(`HTTP ${res.status}`);
        }
        return await res.json();
    }

    async apiPort(port, payload) {
        return await this.apiPost(`/api/port/${port}`, payload);
    }

    async getPort(port) {
        const res = await fetch(`/api/port/${port}`, { cache: "no-store" });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return await res.json();
    }

    log(message, type = "info") {
        if (!this.logBox) return;
        const entry = document.createElement("div");
        entry.className = `log-entry log-${type}`;
        const time = new Date().toLocaleTimeString();
        entry.textContent = `[${time}] ${message}`;
        this.logBox.appendChild(entry);
        this.logBox.scrollTop = this.logBox.scrollHeight;
    }
}

// Instantiate on load
document.addEventListener("DOMContentLoaded", () => {
    window.ev3App = new EV3App();
});
