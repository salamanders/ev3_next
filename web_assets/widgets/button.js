/**
 * # Action Button Widget Module
 *
 * This modular widget renders an interactive push button to control a single motor.
 * It supports three behaviors: momentary hold-to-run, click toggle run/stop, and fixed angle stepping.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/button.js`.
 */

const DEFAULT_SPEED = 600;
const RUN = { speed: DEFAULT_SPEED };
const BRAKE = { command: "stop", stop_action: "brake" };

// Behavior choices displayed in the Design View dropdown.
const ACTIONS = [
    ["momentary", "Hold to Run"],
    ["toggle", "Toggle (Run / Stop)"],
    ["step", "Step Angle"]
];

// Syncs toggle button classes and caption with the motor running state.
function setToggle(btn, running) {
    btn.classList.toggle("toggle-active", running);
    btn.classList.toggle("btn-secondary", !running);
    btn.textContent = running ? "Stop Motor" : "Start Motor";
}

export default {
    type: "button",
    label: "Action Button",
    sizeClass: "run-card-2x1",

    // Configures port A, momentary mode, and 90-degree step by default.
    createDefault(id) {
        return { id, type: "button", port: "A", action: "momentary", degrees: 90 };
    },

    // Renders port, behavior mode, and conditional step-angle inputs.
    renderDesign(w, ctx) {
        return ctx.portField("Motor", "field-port", w.port)
            + ctx.field("Behavior", `<select class="field-action">${ctx.selectOptions(ACTIONS, w.action)}</select>`)
            + (w.action === "step"
                ? ctx.field("Angle (deg)", `<input type="number" class="field-degrees" min="-1080" max="1080" step="15" value="${w.degrees}">`, true)
                : "");
    },

    // Binds port, behavior, and step-angle inputs to widget configuration.
    attachDesign(w, cardElement, ctx) {
        ctx.bindFields(cardElement, w, {
            ".field-port": "port",
            ".field-action": { key: "action", rerender: true },
            ".field-degrees": { key: "degrees", parse: Number }
        });
    },

    // Renders the primary action button styled for the chosen behavior mode.
    renderRun(w) {
        const isToggle = w.action === "toggle";
        const btnClass = isToggle ? "btn-secondary btn-toggle" : "btn-primary";
        const label = isToggle
            ? "Start Motor"
            : (w.action === "step" ? `Rotate ${w.degrees}°` : "Hold to Run");

        return `
            <div class="run-button-box">
                <button class="btn ${btnClass} btn-big-action btn-action-card" id="btn-action-${w.id}">
                    ${label}
                </button>
            </div>
        `;
    },

    // Wires momentary hold, click toggle, or relative-degree step handlers.
    initRun(w, cardElement, ctx) {
        const btn = cardElement.querySelector(`#btn-action-${w.id}`);
        if (!btn) return;
        const sendCmd = (payload) => ctx.apiPost(`/api/port/${w.port}`, payload).catch(() => {});

        if (w.action === "momentary") {
            let active = false;
            const start = (e) => {
                e.preventDefault();
                if (active) return;
                active = true;
                btn.classList.add("btn-danger");
                try { btn.setPointerCapture(e.pointerId); } catch (_) {}
                sendCmd(RUN);
            };
            const stop = (e) => {
                e.preventDefault();
                if (!active) return;
                active = false;
                btn.classList.remove("btn-danger");
                sendCmd(BRAKE);
            };
            btn.addEventListener("pointerdown", start);
            btn.addEventListener("pointerup", stop);
            btn.addEventListener("pointercancel", stop);
            btn.addEventListener("pointerleave", (e) => { if (e.buttons > 0) stop(e); });
        } else if (w.action === "toggle") {
            btn.addEventListener("click", () => {
                const running = !btn.classList.contains("toggle-active");
                setToggle(btn, running);
                sendCmd(running ? RUN : BRAKE);
            });
        } else if (w.action === "step") {
            btn.addEventListener("click", async () => {
                btn.disabled = true;
                const orig = btn.textContent;
                btn.textContent = "Rotating...";
                try {
                    await ctx.apiPost(`/api/port/${w.port}`, {
                        ...RUN,
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

    // Resets toggle button state when global emergency stop is triggered.
    onEmergencyStop(w, cardElement) {
        const btn = cardElement.querySelector(`#btn-action-${w.id}`);
        if (btn && w.action === "toggle") setToggle(btn, false);
    }
};
