/**
 * LEGO Mindstorms EV3 Web Dashboard Controller
 * Enforces strict single-responsibility boundaries and ES2025+ standards.
 */

class WidgetRegistry {
    // Manages dynamic module resolution and runtime widget factories.
    constructor() {
        this.modules = new Map();
        this.manifest = null;
        this.knownTypes = ["joystick", "tank", "slider", "button", "sensor"];
    }

    // Probes local or remote catalog metadata with silent fallback.
    async init() {
        try {
            const res = await fetch(`/widgets/manifest.json?v=${Date.now()}`, {
                signal: AbortSignal.timeout(1500)
            });
            if (res.ok) {
                this.manifest = await res.json();
            }
        } catch (_) {}
    }

    // Resolves cache busting revision token for targeted element type.
    getVersion(type) {
        return this.manifest?.widgets?.[type]?.timestamp
            ?? this.manifest?.timestamp
            ?? 1728670000;
    }

    // Resolves and caches component code and optional styles asynchronously.
    async load(type) {
        if (this.modules.has(type)) {
            return this.modules.get(type);
        }
        const v = this.getVersion(type);
        try {
            const mod = await import(`/widgets/${type}/widget.js?v=${v}`);
            const def = mod.default ?? mod;
            this.modules.set(type, def);

            const files = this.manifest?.widgets?.[type]?.files ?? [];
            if (files.some(f => f.endsWith(".css")) || def.hasCss) {
                const cssId = `widget-css-${type}`;
                if (!document.getElementById(cssId)) {
                    const link = document.createElement("link");
                    link.id = cssId;
                    link.rel = "stylesheet";
                    link.href = `/widgets/${type}/widget.css?v=${v}`;
                    document.head.appendChild(link);
                }
            }

            return def;
        } catch (err) {
            console.error(`Failed loading widget module for '${type}':`, err);
            return null;
        }
    }

    // Concurrently preloads active layout dependencies before render pass.
    async loadActive(types) {
        const unique = [...new Set(types)];
        await Promise.all(unique.map(t => this.load(t)));
    }

    // Preloads catalog components to guarantee rapid interface responses.
    async loadAll() {
        const catalogTypes = Object.keys(this.manifest?.widgets ?? {});
        const allTypes = [...new Set([...this.knownTypes, ...catalogTypes])];
        await Promise.all(allTypes.map(t => this.load(t)));
    }

    // Retrieves cached module definition for synchronous rendering passes.
    get(type) {
        return this.modules.get(type);
    }
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
        this.registry = new WidgetRegistry();

        this.initStorage();
        this.initDOMElements();
        this.attachGlobalListeners();
        this.startPolling();
        this.boot();
    }

    // Distinguishes configuration editing mode from runtime operations.
    get isDesignView() {
        return !!document.getElementById("design-view");
    }

    // Asynchronously loads required widget modules before initial rendering.
    async boot() {
        await this.registry.init();
        await this.registry.loadActive(this.widgets.map(w => w.type));

        if (this.isDesignView) {
            await this.registry.loadAll();
            this.renderHardwarePanel();
            this.rescanHardware(false);
            this.renderDesignWidgets();
        } else {
            this.renderRunWidgets();
        }
    }

    // Persists configuration in local memory with fallback defaults.
    initStorage() {
        try {
            const raw = localStorage.getItem("ev3_dashboard_widgets");
            if (raw) {
                const parsed = JSON.parse(raw);
                if (Array.isArray(parsed) && parsed.length > 0) {
                    const parseMotors = (motors, fallbackPort) => Array.isArray(motors)
                        ? motors.map(m => ({
                            port: String(m.port).trim().toUpperCase(),
                            invert: !!m.invert
                        }))
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
                        if (w.type === "tank") {
                            return {
                                ...base,
                                leftPort: String(w.leftPort ?? "B").trim().toUpperCase(),
                                rightPort: String(w.rightPort ?? "C").trim().toUpperCase(),
                                stopAction: w.stopAction ?? "brake"
                            };
                        }
                        if (w.type === "slider") return { ...base, port: w.port ?? "A" };
                        if (w.type === "button") return {
                            ...base,
                            port: w.port ?? "A",
                            action: w.action ?? "momentary",
                            degrees: w.degrees ?? 90
                        };
                        if (w.type === "sensor") return {
                            ...base,
                            port: w.port ?? "1",
                            mode: w.mode ?? "TOUCH"
                        };
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
            this.dom.btnAdd?.addEventListener("click", async () => {
                const type = this.dom.widgetSelect?.value ?? "joystick";
                await this.addWidget(type);
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

    // Disarms scheduled trailing invocation for a rate-limited action.
    clearThrottle(key) {
        const entry = this.throttleMap.get(key);
        if (entry) {
            if (entry.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }
            entry.nextFn = null;
        }
    }

    // Cancels all pending rate-limited callbacks across the application.
    clearAllThrottles() {
        this.throttleMap.forEach(entry => {
            if (entry.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }
            entry.nextFn = null;
        });
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

    // Transmits coordinated motor targets with link loss timeout protection.
    async apiBatch(commands, heartbeatMs = 2000) {
        return this.apiPost("/api/batch", {
            heartbeat_ms: heartbeatMs,
            commands
        });
    }

    // Immediately halts all active channels and invokes component resets.
    async emergencyStop() {
        try {
            await this.apiPost("/api/estop", {});
            this.clearAllThrottles();

            this.widgets.forEach(w => {
                const def = this.registry.get(w.type);
                const el = document.getElementById(`run-card-${w.id}`);
                if (def?.onEmergencyStop && el) {
                    def.onEmergencyStop(w, el);
                }
            });

            document.querySelectorAll(".btn-toggle").forEach(b => {
                b.className = "btn btn-big-action btn-toggle btn-secondary";
                b.textContent = "Start Motor";
            });
            document.querySelectorAll(".run-range-slider, .run-tank-slider").forEach(s => {
                s.value = "0";
                const readout = s.dataset.side
                    ? document.getElementById(`tank-${s.dataset.side}-val-${s.dataset.id}`)
                    : document.getElementById(`slider-val-${s.dataset.id}`);
                if (readout) readout.textContent = "0";
            });
            document.querySelectorAll(".joystick-knob").forEach(k => {
                k.style.transform = "translate(0px, 0px)";
                k.classList.remove("active");
            });
            document.querySelectorAll(".joystick-readout").forEach(r => {
                r.textContent = "X: 0% | Y: 0%";
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
    async addWidget(type) {
        await this.registry.load(type);
        const def = this.registry.get(type);
        const id = `w_${Date.now()}`;
        const item = def?.createDefault ? def.createDefault(id) : { id, type, port: "A" };

        this.widgets.push(item);
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
        const def = this.registry.get(w.type);
        const title = def?.label ?? w.type;
        const fields = def?.renderDesign ? def.renderDesign(w, this) : "";

        return `
            <div class="design-card" id="card-${w.id}">
                <div class="design-card-header">
                    <div class="design-card-title">
                        <span>${title}</span>
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

        this.widgets.forEach(w => {
            const def = this.registry.get(w.type);
            const cardElement = document.getElementById(`card-${w.id}`);
            if (def?.attachDesign && cardElement) {
                def.attachDesign(w, cardElement, this);
            }
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

    // Constructs operational interface markup for an active item.
    renderRunCard(w) {
        const def = this.registry.get(w.type);
        const sizeClass = def?.sizeClass ?? "run-card-2x1";
        const portBadge = def?.getPortBadge ? def.getPortBadge(w) : (w.port ? `Port ${w.port}` : "");
        const cardTitle = def?.label ?? w.type;
        const portAttr = (w.type === "slider" || w.type === "button")
            ? `data-motor-port="${w.port}"`
            : (w.type === "sensor" ? `data-sensor-port="${w.port}"` : "");
        const body = def?.renderRun ? def.renderRun(w, this) : "";

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
            const def = this.registry.get(w.type);
            const card = document.getElementById(`run-card-${w.id}`);
            if (def?.initRun && card) {
                def.initRun(w, card, this);
            }
        });
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
            if (!m || !m.connected) {
                return `<tr><td><strong>${port}</strong></td><td colspan="5" class="text-muted">Disconnected</td></tr>`;
            }
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

    // Displays numeric metric values and driver metadata on cards.
    updateSensorReadouts(sensors) {
        this.widgets.forEach(w => {
            if (w.type === "sensor") {
                const s = sensors.find(x => x.port === w.port);
                const card = document.getElementById(`run-card-${w.id}`);
                const def = this.registry.get(w.type);
                if (card && def?.onTelemetry) {
                    def.onTelemetry(w, card, s);
                }
            }
        });
    }

    // Synchronizes action widget footers with hardware link states.
    updateMotorCardStatus(motors) {
        document.querySelectorAll(".run-card[data-motor-port]").forEach(card => {
            const port = card.getAttribute("data-motor-port");
            const m = motors.find(x => x.port === port);
            const footer = card.querySelector(".run-card-footer");
            if (footer) {
                footer.textContent = (!m || !m.connected) ? "Disconnected" : (m.state?.length ? m.state.join(", ") : "Ready");
            }
        });
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
