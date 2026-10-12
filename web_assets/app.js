/**
 * # EV3 Web Dashboard Application Controller
 *
 * This script runs in the browser to control LEGO Mindstorms EV3 robots.
 * It manages two distinct screens:
 * 1. Run View (`/` and `/run`): Live operator dashboard displaying interactive control widgets.
 * 2. Design View (`/design`): Visual layout editor for adding widgets, assigning ports, and monitoring diagnostics.
 *
 * `app.js` serves as the central frontend controller. It runs the 250 ms telemetry polling loop,
 * dynamically imports modular widget ES scripts, persists dashboard configuration to localStorage,
 * and transmits command payloads to the backend REST API.
 */

class WidgetRegistry {
    /**
     * Creates a widget registry to cache dynamically imported ES modules.
     *
     * Initializes internal map, manifest cache, and default known widget types.
     *
     * Isolates dynamic module importing and version cache-busting from DOM rendering.
     */
    constructor() {
        this.modules = new Map();
        this.manifest = null;
        this.knownTypes = ["joystick", "tank", "slider", "button", "sensor"];
    }

    /**
     * Fetches widget catalog manifest from the web server.
     *
     * Sends a GET request for `/widgets/manifest.json` with a cache-busting timestamp.
     * Stores the manifest object in `this.manifest`.
     *
     * Called once during application boot to check available widget versions.
     */
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

    /**
     * Resolves the version timestamp for a widget type to bust browser caches.
     *
     * Returns the widget timestamp from the catalog manifest, or fallback timestamp.
     *
     * Ensures updated widget scripts download immediately after over-the-air updates.
     */
    getVersion(type) {
        return this.manifest?.widgets?.[type]?.timestamp
            ?? this.manifest?.timestamp
            ?? 1728670000;
    }

    /**
     * Dynamically imports a widget module script using native ES import.
     *
     * Imports `/widgets/${type}/widget.js?v=${version}` and caches the exported module object.
     *
     * Loads widget code on demand, keeping initial page load fast and lightweight.
     */
    async load(type) {
        if (this.modules.has(type)) {
            return this.modules.get(type);
        }
        const v = this.getVersion(type);
        try {
            const mod = await import(`/widgets/${type}/widget.js?v=${v}`);
            const def = mod.default ?? mod;
            this.modules.set(type, def);
            return def;
        } catch (err) {
            console.error(`Failed loading widget module for '${type}':`, err);
            return null;
        }
    }

    /**
     * Preloads all widget types present in the active user dashboard layout.
     *
     * Deduplicates widget types from the configuration array and loads them concurrently.
     *
     * Avoids render delays by pre-fetching scripts before building DOM cards.
     */
    async loadActive(types) {
        const unique = [...new Set(types)];
        await Promise.all(unique.map(t => this.load(t)));
    }

    /**
     * Preloads all catalog widget types for the visual designer.
     *
     * Combines default types with any extra types discovered in `manifest.json`.
     *
     * Used in Design View so that adding any widget opens instantly without network delay.
     */
    async loadAll() {
        const catalogTypes = Object.keys(this.manifest?.widgets ?? {});
        const allTypes = [...new Set([...this.knownTypes, ...catalogTypes])];
        await Promise.all(allTypes.map(t => this.load(t)));
    }

    /**
     * Returns a cached widget definition.
     *
     * Looks up `type` in the internal Map and returns the exported widget object.
     *
     * Provides synchronous lookups during template rendering.
     */
    get(type) {
        return this.modules.get(type);
    }
}

class EV3App {
    /**
     * Initializes the client application and triggers startup sequence.
     *
     * Initializes state arrays, loads saved widgets from localStorage, binds DOM elements,
     * starts telemetry polling, and boots widget modules.
     *
     * Top-level state coordinator holding active hardware status and dashboard configuration.
     */
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

    /**
     * Checks if current page is Design View or Run View.
     *
     * Returns true if element `#design-view` exists in the DOM.
     *
     * Allows shared `app.js` script to configure page-specific controls.
     */
    get isDesignView() {
        return !!document.getElementById("design-view");
    }

    /**
     * Asynchronously loads widget modules and triggers initial rendering pass.
     *
     * 1. Fetches catalog manifest via `registry.init()`.
     * 2. Preloads active widgets.
     * 3. Populates missing default properties from widget module definitions.
     * 4. Renders Design View or Run View cards.
     *
     * Coordinates asynchronous module loading before attaching DOM event listeners.
     */
    async boot() {
        await this.registry.init();
        await this.registry.loadActive(this.widgets.map(w => w.type));

        // Ensure default properties are populated from widget module definitions
        this.widgets = this.widgets.map(w => {
            const def = this.registry.get(w.type);
            return def?.createDefault ? { ...def.createDefault(w.id), ...w } : w;
        });

        if (this.isDesignView) {
            await this.registry.loadAll();
            this.renderHardwarePanel();
            this.rescanHardware(false);
            this.renderDesignWidgets();
        } else {
            this.renderRunWidgets();
        }
    }

    /**
     * Reads saved widget configuration from browser localStorage.
     *
     * Parses `ev3_dashboard_widgets` from `localStorage`. If empty or corrupt, loads defaults.
     *
     * Preserves user customized dashboard layouts across page refreshes.
     */
    initStorage() {
        try {
            const raw = localStorage.getItem("ev3_dashboard_widgets");
            if (raw) {
                const parsed = JSON.parse(raw);
                if (Array.isArray(parsed) && parsed.length > 0) {
                    this.widgets = parsed.filter(w => w && w.id && w.type);
                    if (this.widgets.length > 0) return;
                }
            }
        } catch (_) {}
        this.loadDefaultWidgets();
    }

    /**
     * Populates default dashboard layout with a joystick and a slider.
     *
     * Sets `this.widgets` to a dual-motor joystick on ports B/C and a speed slider on port A.
     *
     * Provides an out-of-the-box working interface on the first browser visit.
     */
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

    /**
     * Saves active widget layout to browser localStorage.
     *
     * Serializes `this.widgets` to JSON and writes to key `ev3_dashboard_widgets`.
     *
     * Called whenever widgets are added, removed, or reconfigured.
     */
    saveWidgets() {
        try {
            localStorage.setItem("ev3_dashboard_widgets", JSON.stringify(this.widgets));
        } catch (_) {}
    }

    /**
     * Caches frequently queried DOM elements.
     *
     * Stores references to header badges, buttons, lists, and diagnostic tables in `this.dom`.
     *
     * Avoids running expensive `document.getElementById` queries during 4 Hz polling updates.
     */
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

    /**
     * Attaches keyboard and global button event listeners.
     *
     * 1. Binds Spacebar key to `emergencyStop()`.
     * 2. Binds Header E-Stop and Shutdown buttons.
     * 3. In Design View, binds Add Widget, Rescan Hardware, and Clear Log buttons.
     *
     * Central event binding function called during application setup.
     */
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

    /**
     * Limits the frequency of rapid function invocations while guaranteeing the final call runs.
     *
     * Executes `fn` immediately if `intervalMs` has elapsed since the last run.
     * Otherwise schedules a trailing timer so the latest user input (e.g. joystick release) is sent.
     *
     * Prevents pointermove events (60 Hz) from flooding the 300 MHz EV3 CPU with HTTP requests.
     */
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

    /**
     * Cancels any pending scheduled timer for a throttle key.
     *
     * Clears timeout handle and discards pending closure.
     *
     * Called when a control is released or stopped.
     */
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

    /**
     * Cancels all pending throttled actions across the application.
     *
     * Iterates over all keys in `throttleMap` and clears timers.
     *
     * Called by `emergencyStop` to prevent queued move commands from firing after an emergency stop.
     */
    clearAllThrottles() {
        this.throttleMap.forEach(entry => {
            if (entry.timer) {
                clearTimeout(entry.timer);
                entry.timer = null;
            }
            entry.nextFn = null;
        });
    }

    /**
     * Transmits a JSON POST request to a backend API endpoint.
     *
     * Sends `fetch` request with 3-second timeout and unwraps `data` from the `ApiResponse` envelope.
     *
     * Core network transport helper shared by all API calls.
     */
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

    /**
     * Sends a batch motor command payload to `/api/batch`.
     *
     * Calls `apiPost("/api/batch", { heartbeat_ms, commands })`.
     *
     * Used by multi-motor controls (joystick, tank) to synchronize dual-motor actuation.
     */
    async apiBatch(commands, heartbeatMs = 2000) {
        return this.apiPost("/api/batch", {
            heartbeat_ms: heartbeatMs,
            commands
        });
    }

    /**
     * Immediately stops all robot motors.
     *
     * 1. Calls `POST /api/estop`.
     * 2. Clears all queued throttled commands via `clearAllThrottles()`.
     * 3. Invokes `onEmergencyStop()` on all active widget definitions to reset UI controls.
     *
     * Safety action triggered by the header button or the Spacebar key.
     */
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
            this.log("Emergency stop dispatched.", "warn");
        } catch (e) {
            this.log(`Emergency stop failed: ${e.message}`, "error");
        }
    }

    /**
     * Requests clean operating system shutdown of the EV3 brick.
     *
     * Prompts for confirmation and sends `POST /api/shutdown`.
     *
     * Allows safe headless shutdown from the browser without needing an SSH shell.
     */
    async powerOff() {
        if (!confirm("Safely shut down the EV3 brick?")) return;
        try {
            await this.apiPost("/api/shutdown", {});
            alert("EV3 is shutting down. The status light will turn off.");
        } catch (e) {
            alert(`Shutdown request failed: ${e.message}`);
        }
    }

    /**
     * Triggers a hardware port rescan on the EV3 brick.
     *
     * Calls `POST /api/rescan`, updates connected motor/sensor arrays, and re-renders hardware panels.
     *
     * Invoked when new cables are plugged in or when the user clicks "Rescan".
     */
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

    /**
     * Renders connected hardware badges in Design View.
     *
     * Maps ports A..D and 1..4 to green/gray status badges indicating driver name and connection.
     *
     * Visual hardware status display in the Design View sidebar.
     */
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

    /**
     * Builds `<option>` tags for port dropdown selectors.
     *
     * Formats option tags for motor ports (A..D) or sensor ports (1..4) with driver names if connected.
     *
     * Helper used by widget `renderDesign` functions to generate port select dropdowns.
     */
    portOptions(selected, isSensor = false) {
        const ports = isSensor ? ["1", "2", "3", "4"] : ["A", "B", "C", "D"];
        const list = isSensor ? this.hardwareSensors : this.hardwareMotors;
        return ports.map(p => {
            const hw = list.find(x => x.port === p);
            const label = hw?.connected ? `Port ${p} (${hw.driver_name})` : `Port ${p}`;
            return `<option value="${p}" ${selected === p ? "selected" : ""}>${label}</option>`;
        }).join("");
    }

    /**
     * Adds a new widget to the dashboard layout.
     *
     * Loads widget module, calls its `createDefault(id)` factory, appends to `this.widgets`, and saves.
     *
     * Triggered by the "Add Widget" button in Design View.
     */
    async addWidget(type) {
        await this.registry.load(type);
        const def = this.registry.get(type);
        const id = `w_${Date.now()}`;
        const item = def?.createDefault ? def.createDefault(id) : { id, type, port: "A" };

        this.widgets.push(item);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    /**
     * Deletes a widget from the dashboard layout.
     *
     * Filters out the widget with matching `id`, updates localStorage, and re-renders cards.
     *
     * Triggered by the "Remove" button on widget design cards.
     */
    removeWidget(id) {
        this.widgets = this.widgets.filter(w => w.id !== id);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    /**
     * Updates properties on an existing widget configuration.
     *
     * Merges `patch` into the widget matching `id` and saves to localStorage.
     *
     * Called when users modify port assignments or settings in Design View.
     */
    updateWidget(id, patch) {
        const w = this.widgets.find(x => x.id === id);
        if (w) {
            Object.assign(w, patch);
            this.saveWidgets();
        }
    }

    /**
     * Renders all widget configuration cards in Design View.
     *
     * Maps widgets to HTML cards using `renderDesignCard` and attaches change listeners.
     *
     * Top-level layout rendering method for Design View.
     */
    renderDesignWidgets() {
        if (!this.dom.designList) return;
        const countLabel = document.getElementById("widget-count-label");
        if (countLabel) countLabel.textContent = `${this.widgets.length} active`;

        this.dom.designList.innerHTML = this.widgets.map(w => this.renderDesignCard(w)).join("");
        this.attachDesignCardListeners();
    }

    /**
     * Formats HTML markup for a single widget configuration card.
     *
     * Calls the widget definition's `renderDesign(w, this)` hook to generate input fields.
     *
     * Isolates HTML template construction for design cards.
     */
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

    /**
     * Attaches change listeners to inputs on design cards.
     *
     * Binds Remove buttons and delegates to widget `attachDesign(w, cardElement, this)` hooks.
     *
     * Wires interactivity on newly rendered design cards.
     */
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

    /**
     * Renders all active control cards in Run View.
     *
     * Maps widgets to operational cards using `renderRunCard` and attaches interactive controls.
     *
     * Top-level layout rendering method for Run View.
     */
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

    /**
     * Formats HTML markup for a single operational control card.
     *
     * Calls the widget definition's `renderRun(w, this)` hook to generate touch/mouse controls.
     *
     * Isolates HTML template construction for operational control cards.
     */
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

    /**
     * Attaches interactive pointer and button listeners to operational control cards.
     *
     * Calls widget definition's `initRun(w, card, this)` lifecycle hook.
     *
     * Activates joystick touch dragging, slider input events, and button clicks.
     */
    attachRunCardListeners() {
        this.widgets.forEach(w => {
            const def = this.registry.get(w.type);
            const card = document.getElementById(`run-card-${w.id}`);
            if (def?.initRun && card) {
                def.initRun(w, card, this);
            }
        });
    }

    /**
     * Starts the periodic 250 ms telemetry polling heartbeat.
     *
     * 1. Fetches `GET /api/status` every 250 ms (4 Hz).
     * 2. Updates online connection badge and round-trip latency measurement.
     * 3. Updates battery voltage indicator.
     * 4. Updates motor card footers and sensor reading cards.
     * 5. In Design View, refreshes live diagnostics table when expanded.
     *
     * Keeps telemetry fresh and acts as the client heartbeat resetting the 2-second safety watchdog.
     */
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

    /**
     * Updates header online/offline badge and latency display.
     *
     * Toggles `badge-connected` / `badge-disconnected` CSS classes and displays round-trip milliseconds.
     *
     * Called on every poll response or network error.
     */
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

    /**
     * Formats motor telemetry records for the diagnostics table in Design View.
     *
     * Constructs table rows showing speed, position, duty cycle, and state flags for ports A..D.
     *
     * Diagnostic inspector rendered only when the details foldout is open.
     */
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

    /**
     * Dispatches telemetry data to active sensor cards.
     *
     * Calls `onTelemetry(w, card, s)` on the sensor widget definition with matching port data.
     *
     * Feeds real-time sensor measurements to UI readout cards.
     */
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

    /**
     * Updates motor card footers with live state text.
     *
     * Queries cards with `data-motor-port` and updates footer text ("Ready", "running", "Disconnected").
     *
     * Preserves live card status without full HTML re-rendering.
     */
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

    /**
     * Appends a timestamped log entry to the event console box.
     *
     * Creates a log `<div>` element, appends to `#log-box`, and auto-scrolls to the bottom.
     *
     * Provides visual feedback for API failures and emergency stop events.
     */
    log(msg, type = "info") {
        if (!this.dom.logBox) return;
        const entry = document.createElement("div");
        entry.className = `log-entry log-${type}`;
        entry.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
        this.dom.logBox.appendChild(entry);
        this.dom.logBox.scrollTop = this.dom.logBox.scrollHeight;
    }
}

// Global initialization when the browser finishes parsing the HTML document
window.addEventListener("DOMContentLoaded", () => {
    window.ev3App = new EV3App();
});
