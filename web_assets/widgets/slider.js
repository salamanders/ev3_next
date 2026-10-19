/**
 * # Speed Slider Widget Module
 *
 * This modular widget renders an interactive horizontal slider bar to adjust motor speed from -1000 to +1000 ticks/sec.
 * It includes a quick "Zero (Stop)" button to brake the motor immediately.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/slider.js`.
 */

const BRAKE = { command: "stop", stop_action: "brake" };

// Resets the range thumb and numeric readout to zero.
function zeroSlider(w, cardElement) {
    const slider = cardElement.querySelector(`#slider-input-${w.id}`);
    const readout = cardElement.querySelector(`#slider-val-${w.id}`);
    if (slider) slider.value = "0";
    if (readout) readout.textContent = "0";
}

export default {
    type: "slider",
    label: "Speed Slider",
    sizeClass: "run-card-2x1",

    // Assigns motor port A by default when adding a slider widget.
    createDefault(id) {
        return { id, type: "slider", port: "A" };
    },

    // Renders the single motor port dropdown for Design View.
    renderDesign(w, ctx) {
        return ctx.portField("Motor", "field-port", w.port, true);
    },

    // Binds the motor port dropdown to persist selection changes.
    attachDesign(w, cardElement, ctx) {
        ctx.bindFields(cardElement, w, { ".field-port": "port" });
    },

    // Renders the horizontal speed slider and Zero button for Run View.
    renderRun(w) {
        return `
            <div class="run-slider-box">
                <div class="slider-val-row">
                    <span>Speed: <strong class="slider-val-readout" id="slider-val-${w.id}">0</strong> ticks/s</span>
                </div>
                <input type="range" class="run-range-slider" id="slider-input-${w.id}"
                       min="-1000" max="1000" value="0" step="25">
                <div class="slider-actions">
                    <button class="btn btn-sm btn-secondary btn-slider-zero">Zero (Stop)</button>
                </div>
            </div>
        `;
    },

    // Streams 15 Hz speed updates while dragging and brakes on release.
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
            ctx.stopBurst(key, () => send(BRAKE));
        };

        const grab = () => {
            active = true;
            ctx.cancelStopBurst(key);
        };
        slider.addEventListener("pointerdown", grab);
        slider.addEventListener("touchstart", grab, { passive: true });

        slider.addEventListener("input", (e) => {
            const speed = parseInt(e.target.value, 10);
            if (readout) readout.textContent = speed;
            ctx.throttle(key, 66, () => {
                send(speed === 0 ? BRAKE : { speed }).catch(() => {});
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
