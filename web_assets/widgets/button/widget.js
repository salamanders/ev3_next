export default {
    type: "button",
    label: "Action Button",
    sizeClass: "run-card-2x1",

    // Seeds initial layout schema when adding a discrete action button.
    createDefault(id) {
        return { id, type: "button", port: "A", action: "momentary", degrees: 90 };
    },

    getPortBadge(w) {
        return `Port ${w.port}`;
    },

    // Generates configuration controls for behavioral action types.
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

    // Attaches listeners for behavior switches and rotation limits.
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

    // Generates trigger button with state-specific typography.
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

    // Connects touch, toggle, or angular displacement commands to hardware.
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

    // Deactivates running toggles and restores dormant button styling.
    onEmergencyStop(w, cardElement) {
        const btn = cardElement.querySelector(`#btn-action-${w.id}`);
        if (btn && w.action === "toggle") {
            btn.className = "btn btn-big-action btn-toggle btn-secondary";
            btn.textContent = "Start Motor";
        }
    }
};
