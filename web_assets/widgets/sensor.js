/**
 * # Sensor Display Widget Module
 *
 * This modular widget renders live numerical readings from an EV3 sensor connected to ports 1, 2, 3, or 4.
 * It displays real-time values, units, and allows selecting sensor operating modes (e.g. `COL-COLOR`, `US-DIST-CM`).
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `onTelemetry`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/sensor.js`.
 */

const FALLBACK_MODES = ["TOUCH", "COL-COLOR", "US-DIST-CM", "GYRO-ANG"];

export default {
    type: "sensor",
    label: "Sensor Display",
    sizeClass: "run-card-2x1",

    // Configures input port 1 and TOUCH mode by default.
    createDefault(id) {
        return { id, type: "sensor", port: "1", mode: "TOUCH" };
    },

    // Renders sensor port and operating mode selectors for Design View.
    renderDesign(w, ctx) {
        const sensor = ctx.hardwareSensors?.find(s => s.port === w.port);
        const modes = sensor?.modes?.length ? sensor.modes : FALLBACK_MODES;
        const modeOpts = ctx.selectOptions(modes.map(m => [m, m]), w.mode);

        return ctx.portField("Port", "field-sensor-port", w.port, false, true)
            + ctx.field("Reading", `<select class="field-sensor-mode">${modeOpts}</select>`);
    },

    // Binds sensor port and mode selectors and dispatches mode changes to hardware.
    attachDesign(w, cardElement, ctx) {
        ctx.bindFields(cardElement, w, {
            ".field-sensor-port": { key: "port", rerender: true },
            ".field-sensor-mode": {
                key: "mode",
                after: mode => {
                    if (w.port) {
                        ctx.apiPost(`/api/port/${w.port}`, { mode }).catch(err => {
                            ctx.log(`Failed to set sensor mode: ${err.message}`, "error");
                        });
                    }
                }
            }
        });
    },

    // Renders the live numeric value and unit readout for Run View.
    renderRun(w) {
        return `
            <div class="run-sensor-box">
                <div class="sensor-display-val" data-sensor-port="${w.port}" id="sensor-val-${w.id}">--</div>
                <div class="sensor-display-unit text-muted">${w.mode}</div>
            </div>
        `;
    },

    // Updates sensor value, unit label, and footer status on each 4 Hz poll.
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
