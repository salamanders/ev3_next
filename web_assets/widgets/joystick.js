/**
 * # 2D Joystick Widget Module
 *
 * This modular widget renders an interactive 2D analog joystick for robot steering and drive control.
 * It maps X and Y pointer coordinates to motor speed vectors, supporting both single-motor and multi-motor axes.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/joystick.js`.
 */

// Bounds a numeric value between minimum and maximum limits.
const clamp = (v, min, max) => Math.max(min, Math.min(max, v));

// Axis metadata shared by single-motor dropdowns and the multi-motor matrix.
const AXES = [
    { axis: "x", label: "X Axis (Left / Right)" },
    { axis: "y", label: "Y Axis (Forward / Reverse)" }
];

// Formats the X/Y percentage readout string beneath the touch pad.
const readoutText = (x, y) => `X: ${x}% | Y: ${y}%`;

// Normalizes single-motor and multi-motor axis settings into one list shape.
const axisMotors = (w, axis) =>
    w.multiMotor ? (w[`${axis}Motors`] ?? []) : [{ port: w[`${axis}Port`], invert: !!w[`${axis}Invert`] }];

// Scales normalized [-1, 1] coordinates into full-power batch motor speed commands.
function calculateJoystickCommands(w, normX, normY, ctx) {
    const stop_action = w.stopAction || "brake";
    return [["x", normX], ["y", normY]].flatMap(([axis, norm]) =>
        axisMotors(w, axis).map(({ port, invert }) => ({
            port,
            speed: ctx.scaleSpeed(port, norm, invert),
            stop_action
        }))
    );
}

// Returns the movable knob to the center origin and zeroes the text readout.
function centerKnob(w, cardElement) {
    const knob = cardElement.querySelector(`#joy-knob-${w.id}`);
    if (knob) {
        knob.style.transform = "translate(0px, 0px)";
        knob.classList.remove("active");
    }
    const readout = cardElement.querySelector(`#joy-readout-${w.id}`);
    if (readout) readout.textContent = readoutText(0, 0);
}

// Syncs the Invert checkbox state when an Active motor checkbox changes.
function setRowActive(row, active) {
    const invert = row?.querySelector(".field-mm-invert");
    if (invert) {
        invert.disabled = !active;
        if (!active) invert.checked = false;
    }
    row?.querySelector(".multi-motor-invert-label")?.classList.toggle("disabled", !active);
}

export default {
    type: "joystick",
    label: "2D Joystick",
    sizeClass: "run-card-2x2",

    // Supplies default dual-axis port assignments for new or first-boot cards.
    createDefault(id) {
        return {
            id,
            type: "joystick",
            xPort: "B",
            yPort: "C",
            xInvert: false,
            yInvert: false,
            stopAction: "brake",
            multiMotor: false,
            xMotors: [{ port: "B", invert: false }],
            yMotors: [{ port: "C", invert: false }]
        };
    },

    // Summarizes active X and Y motor ports for the Run View card header.
    getPortBadge(w) {
        const formatAxis = axis =>
            axisMotors(w, axis).map(m => m.invert ? `-${m.port}` : m.port).join(", ") || "None";
        return `X: ${formatAxis("x")} | Y: ${formatAxis("y")}`;
    },

    // Builds Design View controls for single-axis or multi-motor assignment.
    renderDesign(w, ctx) {
        const singleMotorFields = AXES
            .map(({ axis, label }) => ctx.motorField(label, `field-${axis}-port`, w[`${axis}Port`], `field-${axis}-invert`, w[`${axis}Invert`]))
            .join("");

        const multiMotorMatrix = `
            <div class="multi-motor-container full-width">
                ${AXES.map(({ axis, label }) => `
                    <div class="multi-motor-section">
                        <span class="multi-motor-axis-title">${label}</span>
                        <div class="multi-motor-grid">
                            ${ctx.motorPorts.map(col => {
                                const entry = (w[`${axis}Motors`] ?? []).find(m => m.port === col);
                                const isActive = !!entry;
                                const data = `data-axis="${axis}" data-port="${col}"`;
                                return `
                                    <div class="multi-motor-row">
                                        <span class="multi-motor-port-tag">${col}</span>
                                        <label class="multi-motor-toggle-label">
                                            <input type="checkbox" class="field-mm-active" ${data} ${isActive ? "checked" : ""}>
                                            <span>Active</span>
                                        </label>
                                        <label class="multi-motor-invert-label ${!isActive ? "disabled" : ""}">
                                            <input type="checkbox" class="field-mm-invert" ${data}
                                                   ${entry?.invert ? "checked" : ""} ${!isActive ? "disabled" : ""}>
                                            <span>Invert</span>
                                        </label>
                                    </div>
                                `;
                            }).join("")}
                        </div>
                    </div>
                `).join("")}
            </div>
        `;

        return `
            <div class="widget-config-field full-width">
                <label class="checkbox-label">
                    <input type="checkbox" class="field-multi-motor" ${w.multiMotor ? "checked" : ""}>
                    <span>Multi-Motor Control</span>
                </label>
            </div>
            ${w.multiMotor ? multiMotorMatrix : singleMotorFields}
            ${ctx.stopActionField(w)}
        `;
    },

    // Wires Design View inputs to persist port, mode, and inversion changes.
    attachDesign(w, cardElement, ctx) {
        const syncSingleAxis = axis => () => {
            const port = w[`${axis}Port`] ?? "B";
            const invert = !!w[`${axis}Invert`];
            ctx.updateWidget(w.id, { [`${axis}Motors`]: [{ port, invert }] });
        };

        ctx.bindFields(cardElement, w, {
            ...Object.fromEntries(AXES.flatMap(({ axis }) => [
                [`.field-${axis}-port`, { key: `${axis}Port`, after: syncSingleAxis(axis) }],
                [`.field-${axis}-invert`, { key: `${axis}Invert`, after: syncSingleAxis(axis) }]
            ])),
            ".field-stop-action": "stopAction",
            ".field-multi-motor": {
                key: "multiMotor",
                rerender: true,
                after: enabled => {
                    if (enabled) {
                        ctx.updateWidget(w.id, {
                            xMotors: w.xMotors?.length ? w.xMotors : [{ port: w.xPort ?? "B", invert: !!w.xInvert }],
                            yMotors: w.yMotors?.length ? w.yMotors : [{ port: w.yPort ?? "C", invert: !!w.yInvert }]
                        });
                    }
                }
            }
        });

        cardElement.querySelectorAll(".field-mm-active").forEach(el => {
            el.addEventListener("change", (e) => {
                const { axis, port } = e.target.dataset;
                const other = axis === "x" ? "y" : "x";
                const isActive = e.target.checked;
                const current = w[`${axis}Motors`] ?? [];
                const list = !isActive
                    ? current.filter(m => m.port !== port)
                    : current.some(m => m.port === port) ? current : [...current, { port, invert: false }];
                const patch = { [`${axis}Motors`]: list };

                if (isActive) {
                    patch[`${other}Motors`] = (w[`${other}Motors`] ?? []).filter(m => m.port !== port);
                    const oppCheckbox = cardElement.querySelector(
                        `.field-mm-active[data-axis="${other}"][data-port="${port}"]`
                    );
                    if (oppCheckbox?.checked) {
                        oppCheckbox.checked = false;
                        setRowActive(oppCheckbox.closest(".multi-motor-row"), false);
                    }
                }

                if (list.length > 0) patch[`${axis}Port`] = list[0].port;
                ctx.updateWidget(w.id, patch);
                setRowActive(el.closest(".multi-motor-row"), isActive);
            });
        });

        cardElement.querySelectorAll(".field-mm-invert").forEach(el => {
            el.addEventListener("change", (e) => {
                const { axis, port } = e.target.dataset;
                const key = `${axis}Motors`;
                const list = (w[key] ?? []).map(m => m.port === port ? { ...m, invert: e.target.checked } : m);
                ctx.updateWidget(w.id, { [key]: list });
            });
        });
    },

    // Renders the circular joystick boundary, crosshairs, and percentage readout.
    renderRun(w) {
        return `
            <div class="joystick-container">
                <div class="joystick-boundary" id="joy-boundary-${w.id}">
                    <div class="joystick-crosshair-x"></div>
                    <div class="joystick-crosshair-y"></div>
                    <div class="joystick-knob" id="joy-knob-${w.id}"></div>
                </div>
                <div class="joystick-readout" id="joy-readout-${w.id}">${readoutText(0, 0)}</div>
            </div>
        `;
    },

    // Captures pointer drags inside the circle and sends 10 Hz batch commands.
    initRun(w, cardElement, ctx) {
        const boundary = cardElement.querySelector(`#joy-boundary-${w.id}`);
        const knob = cardElement.querySelector(`#joy-knob-${w.id}`);
        const readout = cardElement.querySelector(`#joy-readout-${w.id}`);
        if (!boundary || !knob) return;

        const key = `joy_${w.id}`;
        let dragging = false;

        const onPointerMove = (clientX, clientY) => {
            const rect = boundary.getBoundingClientRect();
            const maxRadius = Math.max(30, (rect.width - (knob.offsetWidth || 44)) / 2 || 70);
            const rawX = clientX - (rect.left + rect.width / 2);
            const rawY = clientY - (rect.top + rect.height / 2);
            const angle = Math.atan2(rawY, rawX);
            const clampedDist = Math.min(Math.hypot(rawX, rawY), maxRadius);
            const dx = clampedDist * Math.cos(angle);
            const dy = clampedDist * Math.sin(angle);

            knob.style.transform = `translate(${dx}px, ${dy}px)`;

            // Normalize rawX/rawY against maxRadius; ctx.scaleSpeed saturates the outer 25% (>= 0.75) to 100%
            const normX = clamp(rawX / maxRadius, -1, 1);
            const normY = clamp(-rawY / maxRadius, -1, 1);

            if (readout) {
                readout.textContent = readoutText(ctx.scalePct(normX), ctx.scalePct(normY));
            }

            ctx.throttle(key, 100, () => {
                const commands = calculateJoystickCommands(w, normX, normY, ctx);
                if (commands.length > 0) {
                    ctx.apiBatch(commands).catch(() => {});
                }
            });
        };

        const stopMotion = () => {
            if (!dragging) return;
            dragging = false;
            centerKnob(w, cardElement);

            const commands = calculateJoystickCommands(w, 0, 0, ctx);
            if (commands.length > 0) {
                ctx.stopBurst(key, () => ctx.apiBatch(commands));
            } else {
                ctx.clearThrottle(key);
            }
        };

        boundary.addEventListener("pointerdown", (e) => {
            e.preventDefault();
            dragging = true;
            ctx.cancelStopBurst(key);
            knob.classList.add("active");
            try {
                boundary.setPointerCapture(e.pointerId);
            } catch (_) {}
            onPointerMove(e.clientX, e.clientY);
        });

        boundary.addEventListener("pointermove", (e) => {
            if (dragging) {
                e.preventDefault();
                onPointerMove(e.clientX, e.clientY);
            }
        });

        ["pointerup", "pointercancel", "lostpointercapture"].forEach(evt => {
            boundary.addEventListener(evt, stopMotion);
        });
        window.addEventListener("blur", stopMotion);
    },

    // Snaps the knob home when an emergency stop is triggered.
    onEmergencyStop(w, cardElement) {
        centerKnob(w, cardElement);
    }
};
