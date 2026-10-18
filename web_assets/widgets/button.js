/**
 * # Action Button Widget Module
 *
 * This modular widget renders an interactive push button to control a single motor.
 * It supports three behaviors: momentary hold-to-run, click toggle run/stop, and fixed angle stepping.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/button/widget.js`.
 */

export default {
    type: "button",
    label: "Action Button",
    sizeClass: "run-card-2x1",

    /**
     * Creates default configuration schema when adding an action button.
     *
     * Returns an object configuring port "A", momentary action, and 90 degree step angle.
     *
     * Factory function called by `EV3App::addWidget`.
     */
    createDefault(id) {
        return { id, type: "button", port: "A", action: "momentary", degrees: 90 };
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
     * Renders dropdowns for motor port, behavior mode, and degree angle input.
     *
     * Provides visual configuration controls in Design View.
     */
    renderDesign(w, ctx) {
        return `
            <div class="widget-config-field">
                <label>Motor</label>
                <select class="field-port" data-id="${w.id}">${ctx.portOptions(w.port)}</select>
            </div>
            <div class="widget-config-field">
                <label>Behavior</label>
                <select class="field-action" data-id="${w.id}">
                    <option value="momentary" ${w.action === "momentary" ? "selected" : ""}>Hold to Run</option>
                    <option value="toggle" ${w.action === "toggle" ? "selected" : ""}>Toggle (Run / Stop)</option>
                    <option value="step" ${w.action === "step" ? "selected" : ""}>Step Angle</option>
                </select>
            </div>
            ${w.action === "step" ? `
                <div class="widget-config-field full-width">
                    <label>Angle (deg)</label>
                    <input type="number" class="field-degrees" data-id="${w.id}" min="-1080" max="1080" step="15" value="${w.degrees}">
                </div>
            ` : ""}
        `;
    },

    /**
     * Attaches change event listeners to design inputs.
     *
     * Updates widget properties in `ctx` when the user modifies port, action, or degree settings.
     *
     * Wires interactivity on newly rendered configuration cards.
     */
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { port: e.target.value });
        });
        cardElement.querySelector(".field-action")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { action: e.target.value });
            ctx.renderDesignWidgets();
        });
        cardElement.querySelector(".field-degrees")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { degrees: Number(e.target.value) });
        });
    },

    /**
     * Formats HTML markup for the live button control card in Run View.
     *
     * Generates a large push button styled according to active behavior mode.
     *
     * Provides the touch and click control interface for Run View.
     */
    renderRun(w) {
        const isToggle = w.action === "toggle";
        const isStep = w.action === "step";
        const btnClass = isToggle ? "btn-secondary btn-toggle" : "btn-primary";
        const label = isToggle
            ? "Start Motor"
            : (isStep ? `Rotate ${w.degrees}°` : "Hold to Run");

        return `
            <div class="run-button-box">
                <button class="btn ${btnClass} btn-big-action btn-action-card" id="btn-action-${w.id}" data-id="${w.id}">
                    ${label}
                </button>
            </div>
        `;
    },

    /**
     * Attaches pointer, click, and touch event handlers to the live button.
     *
     * 1. Momentary mode: Runs motor on `pointerdown` and stops on `pointerup`/`pointerleave`.
     * 2. Toggle mode: Toggles between running and stopping motor on each click.
     * 3. Step mode: Sends `run-to-rel-pos` command with configured degrees.
     *
     * Wires live control execution when the card is mounted into the DOM.
     */
    initRun(w, cardElement, ctx) {
        const btn = cardElement.querySelector(`#btn-action-${w.id}`);
        if (!btn) return;
        const defaultSpeed = 600;
        const sendCmd = (payload) => ctx.apiPost(`/api/port/${w.port}`, payload).catch(() => {});

        if (w.action === "momentary") {
            let active = false;
            const start = (e) => {
                e.preventDefault();
                if (active) return;
                active = true;
                btn.classList.add("btn-danger");
                try { btn.setPointerCapture(e.pointerId); } catch (_) {}
                sendCmd({ speed: defaultSpeed });
            };
            const stop = (e) => {
                e.preventDefault();
                if (!active) return;
                active = false;
                btn.classList.remove("btn-danger");
                sendCmd({ command: "stop", stop_action: "brake" });
            };
            btn.addEventListener("pointerdown", start);
            btn.addEventListener("pointerup", stop);
            btn.addEventListener("pointercancel", stop);
            btn.addEventListener("pointerleave", (e) => { if (e.buttons > 0) stop(e); });
        } else if (w.action === "toggle") {
            btn.addEventListener("click", () => {
                const isRunning = btn.classList.toggle("toggle-active");
                btn.classList.toggle("btn-secondary", !isRunning);
                btn.textContent = isRunning ? "Stop Motor" : "Start Motor";
                sendCmd(isRunning ? { speed: defaultSpeed } : { command: "stop", stop_action: "brake" });
            });
        } else if (w.action === "step") {
            btn.addEventListener("click", async () => {
                btn.disabled = true;
                const orig = btn.textContent;
                btn.textContent = "Rotating...";
                try {
                    await ctx.apiPost(`/api/port/${w.port}`, {
                        speed: defaultSpeed,
                        degrees: w.degrees,
                        stop_action: "hold"
                    });
                } finally {
                    setTimeout(() => {
                        btn.disabled = false;
                        btn.textContent = orig;
                    }, 800);
                }
            });
        }
    },

    /**
     * Resets button styling when an emergency stop is triggered.
     *
     * Clears toggle active state and resets button text to "Start Motor".
     *
     * Lifecycle hook invoked by `EV3App::emergencyStop`.
     */
    onEmergencyStop(w, cardElement) {
        const btn = cardElement.querySelector(`#btn-action-${w.id}`);
        if (btn && w.action === "toggle") {
            btn.className = "btn btn-big-action btn-toggle btn-secondary";
            btn.textContent = "Start Motor";
        }
    }
};
