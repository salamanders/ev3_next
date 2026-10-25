/**
 * # Speed Slider Widget Module
 *
 * This modular widget renders an interactive horizontal slider bar to adjust motor speed from -1000 to +1000 ticks/sec.
 * It includes a quick "Zero (Stop)" button to brake the motor immediately.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/slider.js`.
 */

// Resets the range thumb and numeric readout to zero.
function zeroSlider(w, cardElement) {
    const slider = cardElement.querySelector(`#slider-input-${w.id}`);
    const readout = cardElement.querySelector(`#slider-val-${w.id}`);
    if (slider) slider.value = "0";
    if (readout) readout.textContent = "0%";
}

export default {
    type: "slider",
    label: "Speed Slider",
    sizeClass: "run-card-2x1",

    // Assigns motor port A, normal polarity, and brake release by default.
    createDefault(id) {
        return { id, type: "slider", port: "A", invert: false, stopAction: "brake" };
    },

    // Renders motor port, direction inversion, and release action controls.
    renderDesign(w, ctx) {
        return ctx.motorField("Motor", "field-port", w.port, "field-invert", w.invert, true)
            + ctx.stopActionField(w);
    },

    // Binds motor port, direction inversion, and release action inputs.
    attachDesign(w, cardElement, ctx) {
        ctx.bindFields(cardElement, w, {
            ".field-port": "port",
            ".field-invert": "invert",
            ".field-stop-action": "stopAction"
        });
    },

    // Renders the horizontal speed slider and Zero button for Run View.
    renderRun(w) {
        return `
            <div class="run-slider-box">
                <div class="slider-val-row">
                    <span>Power: <strong class="slider-val-readout" id="slider-val-${w.id}">0%</strong></span>
                </div>
                <input type="range" class="run-range-slider" id="slider-input-${w.id}"
                       min="-100" max="100" value="0" step="1">
                <div class="slider-actions">
                    <button class="btn btn-sm btn-secondary btn-slider-zero">Zero (Stop)</button>
                </div>
            </div>
        `;
    },

    // Streams 10 Hz speed updates with outer 25% full-power zones while dragging and stops on release.
    initRun(w, cardElement, ctx) {
        const slider = cardElement.querySelector(`#slider-input-${w.id}`);
        const readout = cardElement.querySelector(`#slider-val-${w.id}`);
        if (!slider) return;

        const key = `slider_${w.id}`;
        const send = payload => ctx.apiPost(`/api/port/${w.port}`, payload);
        let active = false;

        const stopAndCenter = () => {
            active = false;
            zeroSlider(w, cardElement);
            ctx.stopBurst(key, () => send(ctx.stopPayload(w)));
        };

        const grab = () => {
            active = true;
            ctx.cancelStopBurst(key);
        };
        slider.addEventListener("pointerdown", grab);
        slider.addEventListener("touchstart", grab, { passive: true });

        slider.addEventListener("input", (e) => {
            const norm = parseInt(e.target.value, 10) / 100;
            const pct = ctx.scalePct(norm, w.invert);
            const speed = ctx.scaleSpeed(w.port, norm, w.invert);
            if (readout) readout.textContent = `${pct}%`;
            ctx.throttle(key, 100, () => {
                send(speed === 0 ? ctx.stopPayload(w) : { speed }).catch(() => {});
            });
        });

        ["pointerup", "pointercancel", "lostpointercapture", "touchend", "touchcancel", "change"].forEach(evt => {
            slider.addEventListener(evt, stopAndCenter);
        });
        window.addEventListener("blur", () => {
            if (active || slider.value !== "0") stopAndCenter();
        });

        cardElement.querySelector(".btn-slider-zero")?.addEventListener("click", stopAndCenter);
    },

    // Resets the slider thumb and readout when emergency stop is triggered.
    onEmergencyStop(w, cardElement) {
        zeroSlider(w, cardElement);
    }
};
