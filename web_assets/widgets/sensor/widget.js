export default {
    type: "sensor",
    label: "Sensor Display",
    sizeClass: "run-card-2x1",

    // Seeds initial layout schema when adding a telemetry readout.
    createDefault(id) {
        return { id, type: "sensor", port: "1", mode: "TOUCH" };
    },

    getPortBadge(w) {
        return `Port ${w.port}`;
    },

    // Generates port and channel reading mode configuration dropdowns.
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

    // Attaches listeners for channel reassignment and reading mode updates.
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

    // Generates formatted measurement displays with dynamic units.
    renderRun(w) {
        return `
            <div class="run-sensor-box">
                <div class="sensor-display-val" data-sensor-port="${w.port}" id="sensor-val-${w.id}">--</div>
                <div class="sensor-display-unit text-muted">${w.mode}</div>
            </div>
        `;
    },

    // Passive telemetry sink requiring no active user interaction handlers.
    initRun() {},

    // Updates live measurement values and engineering units from background poll.
    onTelemetry(w, cardElement, s) {
        const connected = s?.connected;
        const readout = cardElement.querySelector(".sensor-display-val");
        if (readout) readout.textContent = connected ? (s.value0 ?? s.value ?? "--") : "--";
        const unit = cardElement.querySelector(".sensor-display-unit");
        if (unit) unit.textContent = connected ? (s.units || s.mode) : "";
        const footer = cardElement.querySelector(".run-card-footer");
        if (footer) footer.textContent = connected ? `${s.driver_name} (${s.mode})` : "Disconnected";
    }
};
