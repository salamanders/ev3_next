export default {
    type: "slider",
    label: "Speed Slider",
    sizeClass: "run-card-2x1",

    // Seeds initial layout schema when adding a velocity slider.
    createDefault(id) {
        return { id, type: "slider", port: "A" };
    },

    getPortBadge(w) {
        return `Port ${w.port}`;
    },

    // Generates port assignment dropdown markup for single actuators.
    renderDesign(w, ctx) {
        return `
            <div class="widget-config-field full-width">
                <label>Motor</label>
                <select class="field-port" data-id="${w.id}">${ctx.portOptions(w.port)}</select>
            </div>
        `;
    },

    // Attaches listeners for actuator port selection changes.
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { port: e.target.value });
        });
    },

    // Generates horizontal speed range bar with readout display.
    renderRun(w) {
        return `
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
    },

    // Transmits rate-limited velocity setpoints on range bar adjustments.
    initRun(w, cardElement, ctx) {
        const slider = cardElement.querySelector(`#slider-input-${w.id}`);
        const readout = cardElement.querySelector(`#slider-val-${w.id}`);
        const btnZero = cardElement.querySelector(`.btn-slider-zero[data-id="${w.id}"]`);
        if (!slider) return;

        slider.addEventListener("input", (e) => {
            const speed = parseInt(e.target.value, 10);
            if (readout) readout.textContent = speed;
            ctx.throttle(`slider_${w.id}`, 66, () => {
                const payload = speed === 0
                    ? { command: "stop", stop_action: "brake" }
                    : { speed };
                ctx.apiPost(`/api/port/${w.port}`, payload).catch(() => {});
            });
        });

        btnZero?.addEventListener("click", () => {
            slider.value = "0";
            if (readout) readout.textContent = "0";
            ctx.clearThrottle(`slider_${w.id}`);
            ctx.apiPost(`/api/port/${w.port}`, { command: "stop", stop_action: "brake" }).catch(() => {});
        });
    },

    // Resets slider thumb position and readout value to zero on halt.
    onEmergencyStop(w, cardElement) {
        const slider = cardElement.querySelector(`#slider-input-${w.id}`);
        const readout = cardElement.querySelector(`#slider-val-${w.id}`);
        if (slider) slider.value = "0";
        if (readout) readout.textContent = "0";
    }
};
