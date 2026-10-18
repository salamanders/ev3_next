/**
 * # Sensor Display Widget Module
 *
 * This modular widget renders live numerical readings from an EV3 sensor connected to ports 1, 2, 3, or 4.
 * It displays real-time values, units, and allows selecting sensor operating modes (e.g. `COL-COLOR`, `US-DIST-CM`).
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `onTelemetry`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/sensor/widget.js`.
 */

export default {
    type: "sensor",
    label: "Sensor Display",
    sizeClass: "run-card-2x1",

    /**
     * Creates default configuration schema when adding a sensor widget.
     *
     * Returns an object configuring port "1" and default mode "TOUCH".
     *
     * Factory function called by `EV3App::addWidget`.
     */
    createDefault(id) {
        return { id, type: "sensor", port: "1", mode: "TOUCH" };
    },

    /**
     * Formats port badge label for the widget card header.
     *
     * Returns "Port 1" (or configured sensor port).
     *
     * Displays active port in card headers.
     */
    getPortBadge(w) {
        return `Port ${w.port}`;
    },

    /**
     * Formats HTML markup for the widget settings card in Design View.
     *
     * Renders dropdowns for sensor port selection and available sensor mode options.
     *
     * Provides visual configuration controls in Design View.
     */
    renderDesign(w, ctx) {
        const sensor = ctx.hardwareSensors?.find(s => s.port === w.port);
        const modes = sensor?.modes?.length ? sensor.modes : ["TOUCH", "COL-COLOR", "US-DIST-CM", "GYRO-ANG"];
        const modeOpts = modes.map(m => `<option value="${m}" ${w.mode === m ? "selected" : ""}>${m}</option>`).join("");

        return `
            <div class="widget-config-field">
                <label>Port</label>
                <select class="field-sensor-port" data-id="${w.id}">${ctx.portOptions(w.port, true)}</select>
            </div>
            <div class="widget-config-field">
                <label>Reading</label>
                <select class="field-sensor-mode" data-id="${w.id}">${modeOpts}</select>
            </div>
        `;
    },

    /**
     * Attaches change event listeners to design inputs.
     *
     * Updates port and mode configurations, and sends `POST /api/port/{port}` with `{ mode }` to change hardware mode.
     *
     * Wires interactivity on newly rendered configuration cards.
     */
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-sensor-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { port: e.target.value });
            ctx.renderDesignWidgets();
        });
        cardElement.querySelector(".field-sensor-mode")?.addEventListener("change", (e) => {
            const mode = e.target.value;
            ctx.updateWidget(w.id, { mode });
            if (w.port) {
                ctx.apiPost(`/api/port/${w.port}`, { mode }).catch(err => {
                    ctx.log(`Failed to set sensor mode: ${err.message}`, "error");
                });
            }
        });
    },

    /**
     * Formats HTML markup for the live sensor reading card in Run View.
     *
     * Renders a large numerical display and engineering unit label.
     *
     * Provides the visual readout interface for Run View.
     */
    renderRun(w) {
        return `
            <div class="run-sensor-box">
                <div class="sensor-display-val" data-sensor-port="${w.port}" id="sensor-val-${w.id}">--</div>
                <div class="sensor-display-unit text-muted">${w.mode}</div>
            </div>
        `;
    },

    /**
     * Passive sensor readout requiring no active pointer controls.
     *
     * No-op function satisfying the modular widget lifecycle interface.
     *
     * Interface completeness for `initRun`.
     */
    initRun() {},

    /**
     * Updates live measurement display on each telemetry polling response.
     *
     * Updates value readout text, engineering units, and footer connection status.
     *
     * Lifecycle hook invoked at 4 Hz by `EV3App::updateSensorReadouts`.
     */
    onTelemetry(w, cardElement, data) {
        const s = data?.sensors ? data.sensors.find(x => x.port === w.port) : data;
        const connected = s?.connected;
        const readout = cardElement.querySelector(".sensor-display-val");
        if (readout) readout.textContent = connected ? (s.value0 ?? s.value ?? "--") : "--";
        const unit = cardElement.querySelector(".sensor-display-unit");
        if (unit) unit.textContent = connected ? (s.units || s.mode) : "";
        const footer = cardElement.querySelector(".run-card-footer");
        if (footer) footer.textContent = connected ? `${s.driver_name} (${s.mode})` : "Disconnected";
    }
};
