/**
 * # EV3 Web Dashboard Application Controller
 *
 * This script runs in the browser to control LEGO Mindstorms EV3 robots.
 * It manages two distinct screens:
 * 1. Run View (`/` and `/run`): Live operator dashboard displaying interactive control widgets.
 * 2. Design View (`/design`): Visual layout editor for adding widgets, assigning ports, and monitoring diagnostics.
 *
 * `app.js` serves as the central frontend controller. It runs the non-overlapping 500 ms telemetry polling loop,
 * dynamically imports modular widget ES scripts, persists dashboard configuration to localStorage,
 * and transmits command payloads to the backend REST API.
 */

const MOTOR_PORTS = ["A", "B", "C", "D"];
const SENSOR_PORTS = ["1", "2", "3", "4"];
const STORAGE_KEY = "ev3_dashboard_widgets";

// Strips the ApiResponse wrapper so callers receive raw payload data.
const unwrap = json => json?.data ?? json;

// Builds a single-port stop payload using the widget's configured release action.
const stopPayload = w => ({ command: "stop", stop_action: w?.stopAction || "brake" });

// Maps [-1, 1] input so the outer 25% of travel (|v| >= 0.75) saturates at 100% speed.
function saturateNorm(v, deadzone = 0.05, fullAt = 0.75) {
    const abs = Math.abs(v);
    if (abs <= deadzone) return 0;
    if (abs >= fullAt) return Math.sign(v);
    return Math.sign(v) * ((abs - deadzone) / (fullAt - deadzone));
}

// Fetches a GET endpoint with a timeout and returns the unwrapped JSON body.
async function getJson(url, timeoutMs) {
    const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return unwrap(await res.json());
}

// Toggles inline element visibility without losing stylesheet layout rules.
const setVisible = (el, visible) => {
    if (el) el.style.display = visible ? "" : "none";
};

// Formats motor status flags for card footers and the diagnostics table.
const motorStateText = m =>
    !m?.connected ? "Disconnected" : (m.state?.length ? m.state.join(", ") : "Ready");

// Clears timers, aborts any in-flight request, and drops pending callbacks on a throttle entry.
function disarm(entry) {
    clearTimeout(entry.timer);
    clearTimeout(entry.holdTimer);
    entry.abort?.abort();
    Object.assign(entry, {
        timer: null,
        holdTimer: null,
        nextFn: null,
        holdFn: null,
        holdCount: 0,
        inFlight: false,
        abort: null,
        onSettled: null
    });
}

// Caches dynamically imported widget ES modules separately from DOM state.
class WidgetRegistry {
    constructor() {
        this.modules = new Map();
        this.availableTypes = [];
    }

    // Queries the backend for widget script names present on disk.
    async init() {
        try {
            const list = await getJson("/api/widgets", 1500);
            if (Array.isArray(list)) this.availableTypes = list;
        } catch (err) {
            console.error("Widget discovery failed:", err);
        }
    }

    // Imports a widget module on demand and caches its default export.
    async load(type, bustCache = false) {
        if (!bustCache && this.modules.has(type)) {
            return this.modules.get(type);
        }
        try {
            const suffix = bustCache ? `?t=${Date.now()}` : "";
            const mod = await import(`/widgets/${type}.js${suffix}`);
            const def = mod.default ?? mod;
            this.modules.set(type, def);
            return def;
        } catch (err) {
            console.error(`Failed loading widget module for '${type}':`, err);
            return null;
        }
    }

    // Preloads every discovered widget module so Design View can read labels.
    async loadAll(bustCache = false) {
        if (bustCache) {
            this.modules.clear();
        }
        await Promise.all(this.availableTypes.map(t => this.load(t, bustCache)));
    }

    // Looks up an already-loaded widget module synchronously during rendering.
    get(type) {
        return this.modules.get(type);
    }
}

// Coordinates hardware telemetry, layout storage, and command dispatch.
class EV3App {
    constructor() {
        this.widgets = [];
        this.hardwareMotors = [];
        this.hardwareSensors = [];
        this.motorPorts = MOTOR_PORTS;
        this.throttleMap = new Map();
        this.stopBurstMap = new Map();
        this.cmdSeq = Date.now() * 100;
        this.activeEntry = null;
        this.isOnline = false;
        this.failCount = 0;
        this.pollTimer = null;
        this.isDesignView = window.location.pathname === "/design";
        this.registry = new WidgetRegistry();

        this.initStorage();
        this.initDOMElements();
        this.attachGlobalListeners();
        this.startPolling();
        this.boot();
    }

    // Discovers widget modules, fills default properties, and renders the view.
    async boot() {
        await this.registry.init();
        await this.registry.loadAll();

        this.widgets = this.widgets.map(w => {
            const def = this.registry.get(w.type);
            return def?.createDefault ? { ...def.createDefault(w.id), ...w } : w;
        });
        this.saveWidgets();

        this.renderWidgetSelect();
        this.applyViewMode(this.isDesignView, false);
    }

    // Switches between Design and Run panels without a full page reload.
    applyViewMode(designMode, pushHistory = true) {
        this.isDesignView = designMode;
        if (pushHistory) {
            const targetPath = designMode ? "/design" : "/run";
            if (window.location.pathname !== targetPath) {
                window.history.pushState({}, "", targetPath);
            }
        }

        [this.dom.designView, this.dom.btnRescan, this.dom.btnUpdate].forEach(el => setVisible(el, designMode));
        setVisible(this.dom.runView, !designMode);
        this.dom.navDesign?.classList.toggle("active", designMode);
        this.dom.navRun?.classList.toggle("active", !designMode);

        if (designMode) {
            this.renderWidgetSelect();
            this.renderHardwarePanel();
            this.rescanHardware(false);
            this.renderDesignWidgets();
        } else {
            this.renderRunWidgets();
        }
    }

    // Populates the widget type selector from all discovered modules.
    renderWidgetSelect() {
        if (!this.dom.widgetSelect) return;
        const pairs = this.registry.availableTypes
            .filter(type => this.registry.get(type))
            .map(type => [type, this.registry.get(type).label ?? type]);
        this.dom.widgetSelect.innerHTML = this.selectOptions(pairs, this.dom.widgetSelect.value);
    }

    // Disables a button with a status caption until an async operation completes.
    async withBusyButton(btn, busyLabel, task) {
        const idleLabel = btn?.textContent;
        if (btn) {
            btn.disabled = true;
            btn.textContent = busyLabel;
        }
        try {
            return await task();
        } finally {
            if (btn) {
                btn.disabled = false;
                btn.textContent = idleLabel;
            }
        }
    }

    // Syncs widget scripts from GitHub via POST /update and reloads modules.
    async updateWidgetsFromGithub() {
        await this.withBusyButton(this.dom.btnUpdate, "Updating...", async () => {
            try {
                const res = await this.apiPost("/update", {});
                await this.registry.init();
                await this.registry.loadAll(true);
                this.renderWidgetSelect();
                this.renderDesignWidgets();
                const msg = `Widgets synced (+${res.created ?? 0} new, ~${res.updated ?? 0} updated, -${res.deleted ?? 0} removed).`;
                this.log(msg, "info");
                alert(msg);
            } catch (e) {
                this.log(`Widget update failed: ${e.message}`, "error");
                alert(`Widget update failed: ${e.message}`);
            }
        });
    }

    // Restores the saved widget layout from localStorage or falls back to defaults.
    initStorage() {
        try {
            const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
            if (Array.isArray(parsed)) {
                this.widgets = parsed.filter(w => w?.id && w.type);
                if (this.widgets.length > 0) return;
            }
        } catch (err) {
            console.error("Saved layout is corrupt; loading defaults:", err);
        }
        this.loadDefaultWidgets();
    }

    // Seeds a starter layout; boot() populates full defaults from each module.
    loadDefaultWidgets() {
        this.widgets = [
            { id: "joy", type: "joystick" },
            { id: "slider", type: "slider" }
        ];
        this.saveWidgets();
    }

    // Persists the current widget array to browser localStorage.
    saveWidgets() {
        try {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(this.widgets));
        } catch (err) {
            console.error("Could not save layout:", err);
        }
    }

    // Caches static DOM nodes once at startup to avoid queries during 4 Hz polls.
    initDOMElements() {
        const ids = {
            runView: "run-view",
            designView: "design-view",
            navDesign: "btn-nav-design",
            navRun: "btn-nav-run",
            btnOpenDesign: "btn-open-design",
            designList: "design-widget-list",
            runList: "run-widget-list",
            emptyNotice: "run-empty-notice",
            hardwareList: "hardware-list",
            hwCountBadge: "hw-count-badge",
            widgetCount: "widget-count-label",
            widgetSelect: "widget-type-select",
            btnAdd: "btn-add-widget",
            btnRescan: "btn-rescan",
            batteryVal: "battery-val",
            connBadge: "conn-badge",
            connText: "conn-text",
            latencyVal: "latency-val",
            btnEstop: "btn-estop-header",
            btnShutdown: "btn-shutdown-header",
            btnUpdate: "btn-update-header",
            diagDetails: "diagnostics-details",
            telemetryBody: "telemetry-table-body",
            logBox: "log-box",
            btnClearLog: "btn-clear-log"
        };
        this.dom = Object.fromEntries(
            Object.entries(ids).map(([key, id]) => [key, document.getElementById(id)])
        );
    }

    // Binds Spacebar E-stop, view navigation, and global toolbar buttons.
    attachGlobalListeners() {
        this.dom.btnEstop?.addEventListener("click", () => this.emergencyStop());
        this.dom.btnShutdown?.addEventListener("click", () => this.powerOff());
        this.dom.btnUpdate?.addEventListener("click", () => this.updateWidgetsFromGithub());

        const navTo = designMode => e => {
            e.preventDefault();
            this.applyViewMode(designMode);
        };
        this.dom.navDesign?.addEventListener("click", navTo(true));
        this.dom.btnOpenDesign?.addEventListener("click", navTo(true));
        this.dom.navRun?.addEventListener("click", navTo(false));
        window.addEventListener("popstate", () => {
            this.applyViewMode(window.location.pathname === "/design", false);
        });

        window.addEventListener("keydown", (e) => {
            if (e.code === "Space" && e.target.tagName !== "INPUT") {
                e.preventDefault();
                this.emergencyStop();
            }
        });

        window.addEventListener("pagehide", () => this.emergencyStop());
        document.addEventListener("visibilitychange", () => {
            if (document.hidden) this.emergencyStop();
        });

        this.dom.btnAdd?.addEventListener("click", () => this.addWidget(this.dom.widgetSelect?.value ?? "joystick"));
        this.dom.btnRescan?.addEventListener("click", () => this.rescanHardware(true));
        this.dom.btnClearLog?.addEventListener("click", () => {
            if (this.dom.logBox) this.dom.logBox.innerHTML = "";
        });
    }

    // Rate-limits pointer moves, coalesces to one in-flight request per key, and repeats held commands at 1 Hz.
    throttle(key, intervalMs, fn, repeatMs = 1000) {
        this.cancelStopBurst(key);
        const now = Date.now();
        const entry = this.throttleMap.get(key) ?? {
            last: 0,
            timer: null,
            holdTimer: null,
            nextFn: null,
            holdFn: null,
            holdCount: 0,
            inFlight: false,
            abort: null,
            onSettled: null
        };
        this.throttleMap.set(key, entry);
        entry.nextFn = fn;
        entry.holdFn = fn;
        entry.holdCount = 0;

        const run = toRun => {
            if (!toRun) return;
            entry.last = Date.now();
            this.activeEntry = entry;
            try {
                toRun();
            } finally {
                this.activeEntry = null;
            }
            scheduleHold();
        };

        const flushNext = () => {
            entry.timer = null;
            if (entry.inFlight || !entry.nextFn) return;
            const toRun = entry.nextFn;
            entry.nextFn = null;
            run(toRun);
        };

        entry.onSettled = () => {
            if (!entry.nextFn || entry.timer) return;
            const wait = Math.max(0, intervalMs - (Date.now() - entry.last));
            if (wait === 0) {
                flushNext();
            } else {
                entry.timer = setTimeout(flushNext, wait);
            }
        };

        const scheduleHold = () => {
            clearTimeout(entry.holdTimer);
            entry.holdTimer = null;
            if (!repeatMs || repeatMs <= 0) return;
            entry.holdTimer = setTimeout(() => {
                entry.holdTimer = null;
                if (entry.inFlight) {
                    scheduleHold();
                    return;
                }
                // Cap stationary hold repeats at 60 (~60 s) so a missed release eventually stops.
                if (entry.holdFn && entry.holdCount < 60) {
                    entry.holdCount += 1;
                    run(entry.holdFn);
                }
            }, repeatMs);
        };

        if (!entry.inFlight && now - entry.last >= intervalMs) {
            clearTimeout(entry.timer);
            entry.timer = null;
            entry.nextFn = null;
            run(fn);
        } else if (!entry.inFlight && !entry.timer) {
            clearTimeout(entry.holdTimer);
            entry.holdTimer = null;
            entry.timer = setTimeout(flushNext, intervalMs - (now - entry.last));
        }
    }

    // Disarms pending move and hold timers for a single control key.
    clearThrottle(key) {
        const entry = this.throttleMap.get(key);
        if (entry) disarm(entry);
    }

    // Disarms all throttled move commands and active stop bursts on E-stop.
    clearAllThrottles() {
        this.throttleMap.forEach(disarm);
        [...this.stopBurstMap.keys()].forEach(key => this.cancelStopBurst(key));
    }

    // Aborts an in-flight stop retry burst when the user touches a control again.
    cancelStopBurst(key) {
        const prev = this.stopBurstMap.get(key);
        if (prev) {
            prev.cancelled = true;
            clearTimeout(prev.timer);
            prev.abort?.abort();
            this.stopBurstMap.delete(key);
        }
    }

    // Sends an immediate stop command plus one confirmation retry on release.
    stopBurst(key, actionFn, maxCount = 2, gapMs = 120) {
        this.clearThrottle(key);
        this.cancelStopBurst(key);

        const state = { cancelled: false, timer: null, inFlight: false, abort: null, onSettled: null };
        this.stopBurstMap.set(key, state);
        let sent = 0;

        const step = async () => {
            if (state.cancelled) return;
            sent += 1;
            let failed = false;
            try {
                this.activeEntry = state;
                let p;
                try {
                    p = actionFn();
                } finally {
                    this.activeEntry = null;
                }
                await p;
            } catch (err) {
                failed = true;
                console.warn(`Stop retry ${sent} for '${key}' failed:`, err);
            }
            if (state.cancelled) return;
            if (sent < maxCount) {
                state.timer = setTimeout(step, failed ? 0 : gapMs);
            } else if (this.stopBurstMap.get(key) === state) {
                this.stopBurstMap.delete(key);
            }
        };

        step();
    }

    // Sends a JSON POST request with sequence numbering, abort tracking, and an 800 ms motor timeout.
    async apiPost(endpoint, payload, timeoutMs) {
        const entry = this.activeEntry;
        const isMotorCmd = endpoint === "/api/batch" || endpoint.startsWith("/api/port/");
        const isFastCmd = isMotorCmd || endpoint === "/api/estop";
        const bodyObj = isMotorCmd ? { ...payload, seq: ++this.cmdSeq } : payload;

        const ctrl = new AbortController();
        if (entry) {
            entry.inFlight = true;
            entry.abort = ctrl;
        }
        const timer = setTimeout(() => ctrl.abort(), timeoutMs ?? (isFastCmd ? 800 : 3000));
        try {
            const res = await fetch(endpoint, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(bodyObj),
                signal: ctrl.signal
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            return unwrap(await res.json().catch(() => ({})));
        } finally {
            clearTimeout(timer);
            if (entry) {
                if (entry.abort === ctrl) entry.abort = null;
                entry.inFlight = false;
                entry.onSettled?.();
            }
        }
    }

    // Dispatches a multi-port command batch with a watchdog heartbeat timeout.
    async apiBatch(commands, heartbeatMs = 5000) {
        return this.apiPost("/api/batch", {
            heartbeat_ms: heartbeatMs,
            commands
        });
    }

    // Invokes a named lifecycle hook on every widget with a mounted card.
    callWidgetHook(hook, idPrefix, ...args) {
        this.widgets.forEach(w => {
            const def = this.registry.get(w.type);
            const card = document.getElementById(`${idPrefix}${w.id}`);
            if (card && def?.[hook]) def[hook](w, card, ...args);
        });
    }

    // Halts all motors immediately, clears throttles, and resets widget UIs.
    async emergencyStop() {
        this.clearAllThrottles();
        this.callWidgetHook("onEmergencyStop", "run-card-");
        try {
            await this.apiPost("/api/estop", {});
            this.log("Emergency stop dispatched.", "warn");
        } catch (e) {
            this.log(`Emergency stop failed: ${e.message}`, "error");
        }
    }

    // Confirms with the operator and requests a clean OS poweroff.
    async powerOff() {
        if (!confirm("Safely shut down the EV3 brick?")) return;
        try {
            await this.apiPost("/api/shutdown", {});
            alert("EV3 is shutting down. The status light will turn off.");
        } catch (e) {
            alert(`Shutdown request failed: ${e.message}`);
        }
    }

    // Re-enumerates sysfs motor and sensor ports and refreshes Design View.
    async rescanHardware(interactive = false) {
        await this.withBusyButton(this.dom.btnRescan, "Scanning...", async () => {
            try {
                const data = await this.apiPost("/api/rescan", {});
                this.hardwareMotors = data.motors ?? [];
                this.hardwareSensors = data.sensors ?? [];
                this.renderHardwarePanel();
                this.renderDesignWidgets();
                if (interactive) this.log("Hardware scan complete.", "info");
            } catch (e) {
                this.log(`Rescan failed: ${e.message}`, "error");
            }
        });
    }

    // Renders connection status cards for all 8 motor and sensor ports.
    renderHardwarePanel() {
        if (!this.dom.hardwareList) return;
        const slots = [
            ...MOTOR_PORTS.map(port => this.hardwareMotors.find(m => m.port === port) ?? { port }),
            ...SENSOR_PORTS.map(port => this.hardwareSensors.find(s => s.port === port) ?? { port })
        ];

        this.dom.hardwareList.innerHTML = slots.map(({ port, connected, driver_name, mode }) => `
            <div class="hw-item ${connected ? "active" : ""}">
                <div class="hw-item-header">
                    <span class="hw-port-label">Port ${port}</span>
                    <span class="badge ${connected ? "badge-connected" : "badge-disconnected"}">${connected ? (mode || "Connected") : "None"}</span>
                </div>
                <div class="hw-driver-name">${connected ? driver_name : "Disconnected"}</div>
            </div>
        `).join("");
        if (this.dom.hwCountBadge) {
            this.dom.hwCountBadge.textContent = `${slots.filter(s => s.connected).length} / ${slots.length} Connected`;
        }
    }

    // Builds <option> tags from [value, label] pairs with selection state.
    selectOptions(pairs, selected) {
        return pairs.map(([value, label = value]) =>
            `<option value="${value}" ${value === selected ? "selected" : ""}>${label}</option>`
        ).join("");
    }

    // Builds <option> tags for motor (A..D) or sensor (1..4) port selectors.
    portOptions(selected, isSensor = false) {
        const [ports, list] = isSensor
            ? [SENSOR_PORTS, this.hardwareSensors]
            : [MOTOR_PORTS, this.hardwareMotors];
        return this.selectOptions(ports.map(p => {
            const hw = list.find(x => x.port === p);
            return [p, hw?.connected ? `Port ${p} (${hw.driver_name})` : `Port ${p}`];
        }), selected);
    }

    // Wraps a label and form control in a standard Design View grid cell.
    field(label, control, fullWidth = false) {
        return `
            <div class="widget-config-field${fullWidth ? " full-width" : ""}">
                <label>${label}</label>
                ${control}
            </div>
        `;
    }

    // Renders a labelled port selector dropdown for motor or sensor widgets.
    portField(label, cssClass, selected, fullWidth = false, isSensor = false) {
        return this.field(label, `<select class="${cssClass}">${this.portOptions(selected, isSensor)}</select>`, fullWidth);
    }

    // Renders a motor port selector paired with an inline Invert direction checkbox.
    motorField(label, portClass, selectedPort, invertClass, inverted = false, fullWidth = false) {
        const control = `
            <div class="motor-port-row">
                <select class="${portClass}">${this.portOptions(selectedPort)}</select>
                <label class="multi-motor-invert-label">
                    <input type="checkbox" class="${invertClass}" ${inverted ? "checked" : ""}>
                    <span>Invert</span>
                </label>
            </div>
        `;
        return this.field(label, control, fullWidth);
    }

    // Renders the shared Brake / Coast release selector for drive widgets.
    stopActionField(w) {
        const options = this.selectOptions([
            ["brake", "Lock / Brake (Hold Position)"],
            ["coast", "Coast (Free Spin)"]
        ], w.stopAction || "brake");
        return this.field("Release Action", `<select class="field-stop-action">${options}</select>`, true);
    }

    // Builds a single-port stop payload using the widget's configured release action.
    stopPayload(w) {
        return stopPayload(w);
    }

    // Looks up a motor port's hardware max_speed (1050 Large, 1560 Medium; fallback 1560 clamped by backend).
    maxSpeed(port) {
        return this.hardwareMotors?.find(m => m.port === port && m.connected)?.max_speed || 1560;
    }

    // Scales a normalized [-1, 1] control position with outer-edge 100% saturation into motor speed ticks/s.
    scaleSpeed(port, norm, invert = false) {
        const sat = saturateNorm(invert ? -norm : norm);
        return Math.round(sat * this.maxSpeed(port));
    }

    // Converts a normalized [-1, 1] input into a saturated [-100, 100] percentage for UI readouts.
    scalePct(norm, invert = false) {
        return Math.round(saturateNorm(invert ? -norm : norm) * 100);
    }

    // Binds change listeners from a selector-to-property specification map.
    bindFields(card, w, spec) {
        Object.entries(spec).forEach(([selector, opt]) => {
            const { key, parse = v => v, rerender = false, after } = typeof opt === "string" ? { key: opt } : opt;
            card.querySelector(selector)?.addEventListener("change", e => {
                const raw = e.target.type === "checkbox" ? e.target.checked : e.target.value;
                const val = parse(raw);
                this.updateWidget(w.id, { [key]: val });
                if (rerender) this.renderDesignWidgets();
                after?.(val);
            });
        });
    }

    // Instantiates a new widget from its module default factory and saves it.
    async addWidget(type) {
        const def = await this.registry.load(type);
        const id = `w_${Date.now()}`;
        const item = def?.createDefault ? def.createDefault(id) : { id, type, port: "A" };

        this.widgets.push(item);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    // Deletes a widget by ID and re-renders the Design View card list.
    removeWidget(id) {
        this.widgets = this.widgets.filter(w => w.id !== id);
        this.saveWidgets();
        this.renderDesignWidgets();
    }

    // Merges a property patch into a widget configuration and persists it.
    updateWidget(id, patch) {
        const w = this.widgets.find(x => x.id === id);
        if (w) {
            Object.assign(w, patch);
            this.saveWidgets();
        }
    }

    // Renders all Design View configuration cards and wires their inputs.
    renderDesignWidgets() {
        if (!this.dom.designList) return;
        if (this.dom.widgetCount) this.dom.widgetCount.textContent = `${this.widgets.length} active`;

        this.dom.designList.innerHTML = this.widgets.map(w => this.renderDesignCard(w)).join("");
        this.dom.designList.querySelectorAll(".btn-remove").forEach(b => {
            b.addEventListener("click", () => this.removeWidget(b.dataset.id));
        });
        this.callWidgetHook("attachDesign", "card-", this);
    }

    // Builds the outer Design View card shell around a widget's form fields.
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

    // Renders all active Run View control cards and initializes their handlers.
    renderRunWidgets() {
        if (!this.dom.runList) return;
        setVisible(this.dom.emptyNotice, this.widgets.length === 0);
        this.dom.runList.innerHTML = this.widgets.map(w => this.renderRunCard(w)).join("");
        this.callWidgetHook("initRun", "run-card-", this);
    }

    // Builds the outer Run View card shell with port badge and status footer.
    renderRunCard(w) {
        const def = this.registry.get(w.type);
        const sizeClass = def?.sizeClass ?? "run-card-2x1";
        const defaultBadge = w.port ? `Port ${w.invert ? "-" : ""}${w.port}` : "";
        const portBadge = def?.getPortBadge ? def.getPortBadge(w) : defaultBadge;
        const cardTitle = def?.label ?? w.type;
        const portAttr = MOTOR_PORTS.includes(w.port) ? `data-motor-port="${w.port}"` : "";
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

    // Sequentially polls GET /api/status with a 500 ms gap so requests never overlap.
    startPolling() {
        clearTimeout(this.pollTimer);
        const tick = async () => {
            const t0 = performance.now();
            try {
                const data = await getJson("/api/status", 2000);
                this.failCount = 0;
                this.setConnectionState(true, Math.round(performance.now() - t0));

                const volt = data.battery?.voltage_v ?? data.battery_voltage;
                if (this.dom.batteryVal && volt != null) {
                    this.dom.batteryVal.textContent = Number(volt).toFixed(1);
                }

                if (data.motors) {
                    this.hardwareMotors = data.motors;
                    this.updateMotorCardStatus(data.motors);
                    if (this.isDesignView && this.dom.diagDetails?.open) {
                        this.renderDiagnosticsTable(data.motors);
                    }
                }

                if (data.sensors) {
                    this.hardwareSensors = data.sensors;
                }

                this.callWidgetHook("onTelemetry", "run-card-", data);
            } catch (_) {
                this.failCount += 1;
                if (this.failCount >= 2) {
                    this.setConnectionState(false, 0);
                }
            } finally {
                this.pollTimer = setTimeout(tick, 500);
            }
        };
        tick();
    }

    // Updates the footer connection badge and round-trip latency readout.
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

    // Populates the Design View motor telemetry table when expanded.
    renderDiagnosticsTable(motors) {
        if (!this.dom.telemetryBody) return;
        this.dom.telemetryBody.innerHTML = MOTOR_PORTS.map(port => {
            const m = motors.find(x => x.port === port);
            if (!m?.connected) {
                return `<tr><td><strong>${port}</strong></td><td colspan="5" class="text-muted">Disconnected</td></tr>`;
            }
            return `
                <tr>
                    <td><strong>${port}</strong></td>
                    <td><span class="badge ${m.state?.length ? "badge-running" : "badge-ready"}">${motorStateText(m)}</span></td>
                    <td>${m.speed}</td>
                    <td>${m.position}</td>
                    <td>${m.duty_cycle}%</td>
                    <td>${m.polarity}</td>
                </tr>
            `;
        }).join("");
    }

    // Updates Run View card footers with live motor state flags.
    updateMotorCardStatus(motors) {
        document.querySelectorAll(".run-card[data-motor-port]").forEach(card => {
            const footer = card.querySelector(".run-card-footer");
            if (footer) {
                footer.textContent = motorStateText(motors.find(x => x.port === card.dataset.motorPort));
            }
        });
    }

    // Appends a timestamped diagnostic message to the Design View console.
    log(msg, type = "info") {
        if (!this.dom.logBox) return;
        const entry = document.createElement("div");
        entry.className = `log-entry log-${type}`;
        entry.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
        this.dom.logBox.appendChild(entry);
        this.dom.logBox.scrollTop = this.dom.logBox.scrollHeight;
    }
}

window.addEventListener("DOMContentLoaded", () => {
    window.ev3App = new EV3App();
});
