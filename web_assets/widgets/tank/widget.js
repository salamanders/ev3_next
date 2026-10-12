/**
 * # Tank Drive Controls Widget Module
 *
 * This modular widget renders twin vertical sliders for differential tank-steering robots.
 * It provides independent speed control for left and right drive tracks with a shared "Stop Both" button.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/tank/widget.js`.
 */

export default {
    type: "tank",
    label: "Tank Controls",
    sizeClass: "run-card-2x2",

    /**
     * Creates default configuration schema when adding a tank controls widget.
     *
     * Returns an object assigning port B to left track and port C to right track.
     *
     * Factory function called by `EV3App::addWidget`.
     */
    createDefault(id) {
        return {
            id,
            type: "tank",
            leftPort: "B",
            rightPort: "C",
            stopAction: "brake"
        };
    },

    /**
     * Formats port badge label for the widget card header.
     *
     * Returns `L: B | R: C` (or configured track ports).
     *
     * Displays active track ports in card headers.
     */
    getPortBadge(w) {
        return `L: ${w.leftPort} | R: ${w.rightPort}`;
    },

    /**
     * Formats HTML markup for the widget settings card in Design View.
     *
     * Renders dropdown selectors for left track port, right track port, and release stop action.
     *
     * Provides visual configuration controls in Design View.
     */
    renderDesign(w, ctx) {
        return `
            <div class="widget-config-field">
                <label>Left Track</label>
                <select class="field-left-port" data-id="${w.id}">${ctx.portOptions(w.leftPort)}</select>
            </div>
            <div class="widget-config-field">
                <label>Right Track</label>
                <select class="field-right-port" data-id="${w.id}">${ctx.portOptions(w.rightPort)}</select>
            </div>
            <div class="widget-config-field full-width">
                <label>Release Action</label>
                <select class="field-stop-action" data-id="${w.id}">
                    <option value="brake" ${w.stopAction === "brake" ? "selected" : ""}>Brake (Hold Position)</option>
                    <option value="coast" ${w.stopAction === "coast" ? "selected" : ""}>Coast (Free Spin)</option>
                </select>
            </div>
        `;
    },

    /**
     * Attaches change event listeners to design inputs.
     *
     * Updates widget properties in `ctx` when the user selects different left/right track ports or stop action.
     *
     * Wires interactivity on newly rendered configuration cards.
     */
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-left-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { leftPort: e.target.value });
        });
        cardElement.querySelector(".field-right-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { rightPort: e.target.value });
        });
        cardElement.querySelector(".field-stop-action")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { stopAction: e.target.value });
        });
    },

    /**
     * Formats HTML markup for the live dual-slider control card in Run View.
     *
     * Renders side-by-side vertical sliders for left and right tracks, speed readouts, and Stop Both button.
     *
     * Provides the touch and slider control interface for Run View.
     */
    renderRun(w) {
        return `
            <div class="tank-controls-container">
                <div class="tank-track-col">
                    <span class="tank-track-title">Left (${w.leftPort})</span>
                    <div class="tank-slider-wrapper">
                        <input type="range" class="run-tank-slider" id="tank-left-${w.id}"
                               min="-1000" max="1000" value="0" step="25" data-id="${w.id}" data-side="left"
                               orient="vertical">
                    </div>
                    <span class="tank-speed-readout" id="tank-left-val-${w.id}">0</span>
                </div>
                <div class="tank-track-divider"></div>
                <div class="tank-track-col">
                    <span class="tank-track-title">Right (${w.rightPort})</span>
                    <div class="tank-slider-wrapper">
                        <input type="range" class="run-tank-slider" id="tank-right-${w.id}"
                               min="-1000" max="1000" value="0" step="25" data-id="${w.id}" data-side="right"
                               orient="vertical">
                    </div>
                    <span class="tank-speed-readout" id="tank-right-val-${w.id}">0</span>
                </div>
            </div>
            <div class="tank-actions">
                <button class="btn btn-sm btn-secondary btn-tank-zero" data-id="${w.id}">Stop Both</button>
            </div>
        `;
    },

    /**
     * Attaches input event listeners to the vertical sliders and Stop Both button.
     *
     * 1. On slider adjustments: sends throttled synchronized batch commands to `/api/batch` at 15 Hz.
     * 2. On Stop Both button click: resets both sliders to 0 and sends batch stop command with chosen stop action.
     *
     * Wires live control execution when the card is mounted into the DOM.
     */
    initRun(w, cardElement, ctx) {
        const leftSlider = cardElement.querySelector(`#tank-left-${w.id}`);
        const rightSlider = cardElement.querySelector(`#tank-right-${w.id}`);
        const leftVal = cardElement.querySelector(`#tank-left-val-${w.id}`);
        const rightVal = cardElement.querySelector(`#tank-right-val-${w.id}`);
        const btnZero = cardElement.querySelector(`.btn-tank-zero[data-id="${w.id}"]`);
        if (!leftSlider || !rightSlider) return;

        const updateSpeeds = () => {
            const leftSpeed = parseInt(leftSlider.value, 10);
            const rightSpeed = parseInt(rightSlider.value, 10);
            if (leftVal) leftVal.textContent = leftSpeed;
            if (rightVal) rightVal.textContent = rightSpeed;

            ctx.throttle(`tank_${w.id}`, 66, () => {
                const stopAction = w.stopAction || "brake";
                const commands = [
                    { port: w.leftPort, speed: leftSpeed, stop_action: stopAction },
                    { port: w.rightPort, speed: rightSpeed, stop_action: stopAction }
                ];
                ctx.apiBatch(commands).catch(() => {});
            });
        };

        leftSlider.addEventListener("input", updateSpeeds);
        rightSlider.addEventListener("input", updateSpeeds);

        btnZero?.addEventListener("click", () => {
            leftSlider.value = "0";
            rightSlider.value = "0";
            if (leftVal) leftVal.textContent = "0";
            if (rightVal) rightVal.textContent = "0";
            ctx.clearThrottle(`tank_${w.id}`);
            const stopAction = w.stopAction || "brake";
            ctx.apiBatch([
                { port: w.leftPort, speed: 0, stop_action: stopAction },
                { port: w.rightPort, speed: 0, stop_action: stopAction }
            ]).catch(() => {});
        });
    },

    /**
     * Resets both track slider values and text readouts to zero on emergency stop.
     *
     * Sets left and right slider inputs to "0" and text readouts to "0".
     *
     * Lifecycle hook invoked by `EV3App::emergencyStop`.
     */
    onEmergencyStop(w, cardElement) {
        const leftSlider = cardElement.querySelector(`#tank-left-${w.id}`);
        const rightSlider = cardElement.querySelector(`#tank-right-${w.id}`);
        const leftVal = cardElement.querySelector(`#tank-left-val-${w.id}`);
        const rightVal = cardElement.querySelector(`#tank-right-val-${w.id}`);
        if (leftSlider) leftSlider.value = "0";
        if (rightSlider) rightSlider.value = "0";
        if (leftVal) leftVal.textContent = "0";
        if (rightVal) rightVal.textContent = "0";
    }
};
