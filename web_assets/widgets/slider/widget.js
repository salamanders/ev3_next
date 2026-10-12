/**
 * # Speed Slider Widget Module
 *
 * This modular widget renders an interactive horizontal slider bar to adjust motor speed from -1000 to +1000 ticks/sec.
 * It includes a quick "Zero (Stop)" button to brake the motor immediately.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/slider/widget.js`.
 */

export default {
    type: "slider",
    label: "Speed Slider",
    sizeClass: "run-card-2x1",

    /**
     * Creates default configuration schema when adding a slider widget.
     *
     * Returns an object configuring motor port "A".
     *
     * Factory function called by `EV3App::addWidget`.
     */
    createDefault(id) {
        return { id, type: "slider", port: "A" };
    },

    /**
     * Formats port badge label for the widget card header.
     *
     * Returns "Port A" (or configured motor port).
     *
     * Displays active port in card headers.
     */
    getPortBadge(w) {
        return `Port ${w.port}`;
    },

    /**
     * Formats HTML markup for the widget settings card in Design View.
     *
     * Renders motor port selection dropdown.
     *
     * Provides visual configuration controls in Design View.
     */
    renderDesign(w, ctx) {
        return `
            <div class="widget-config-field full-width">
                <label>Motor</label>
                <select class="field-port" data-id="${w.id}">${ctx.portOptions(w.port)}</select>
            </div>
        `;
    },

    /**
     * Attaches change event listener to port selection input.
     *
     * Updates widget `port` in `ctx` when the user selects a different motor.
     *
     * Wires interactivity on newly rendered configuration cards.
     */
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { port: e.target.value });
        });
    },

    /**
     * Formats HTML markup for the live slider control card in Run View.
     *
     * Renders a range slider from -1000 to +1000 with step 25, speed readout, and Zero button.
     *
     * Provides the touch and slider control interface for Run View.
     */
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

    /**
     * Attaches input event listeners to the range slider and Zero button.
     *
     * 1. On slider drag: throttles speed commands at 15 Hz (66 ms) via `POST /api/port/{port}`.
     * 2. When slider is set to 0: sends stop command with brake action.
     * 3. On Zero button click: resets slider value to 0, clears throttle timer, and stops motor.
     *
     * Wires live control execution when the card is mounted into the DOM.
     */
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

    /**
     * Resets slider position and text readout to zero on emergency stop.
     *
     * Sets slider input value to "0" and text readout to "0".
     *
     * Lifecycle hook invoked by `EV3App::emergencyStop`.
     */
    onEmergencyStop(w, cardElement) {
        const slider = cardElement.querySelector(`#slider-input-${w.id}`);
        const readout = cardElement.querySelector(`#slider-val-${w.id}`);
        if (slider) slider.value = "0";
        if (readout) readout.textContent = "0";
    }
};
