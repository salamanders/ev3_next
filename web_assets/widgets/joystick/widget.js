// Pure boundary arithmetic and multi-channel velocity setpoint calculation.
const clamp = (v, min, max) => Math.max(min, Math.min(max, v));

function getJoystickPorts(w) {
    return !w.multiMotor
        ? [w.xPort, w.yPort]
        : [...new Set([...(w.xMotors ?? []).map(m => m.port), ...(w.yMotors ?? []).map(m => m.port)])];
}

function calculateJoystickCommands(w, normX, normY, maxSpeed = 1000) {
    const stopAction = w.stopAction || "brake";
    const xSpeed = clamp(Math.round(normX * maxSpeed), -maxSpeed, maxSpeed);
    const ySpeed = clamp(Math.round(normY * maxSpeed), -maxSpeed, maxSpeed);

    if (!w.multiMotor) {
        return [
            { port: w.xPort, speed: xSpeed, stop_action: stopAction },
            { port: w.yPort, speed: ySpeed, stop_action: stopAction }
        ];
    }

    const mapMotors = (motors, speed) => (motors ?? []).map(m => ({
        port: m.port,
        speed: m.invert ? -speed : speed,
        stop_action: stopAction
    }));
    return [...mapMotors(w.xMotors, xSpeed), ...mapMotors(w.yMotors, ySpeed)];
}

function getJoystickStopCommands(w) {
    const stopAction = w.stopAction || "brake";
    return getJoystickPorts(w).map(port => ({ port, speed: 0, stop_action: stopAction }));
}

function formatPortBadge(w) {
    if (!w.multiMotor) {
        return `X: ${w.xPort} | Y: ${w.yPort}`;
    }
    const formatAxis = (motors) => {
        if (!motors || motors.length === 0) return "None";
        return motors.map(m => m.invert ? `-${m.port}` : m.port).join(", ");
    };
    return `X: ${formatAxis(w.xMotors)} | Y: ${formatAxis(w.yMotors)}`;
}

export default {
    type: "joystick",
    label: "2D Joystick",
    sizeClass: "run-card-2x2",

    // Seeds initial layout schema when adding a new directional control.
    createDefault(id) {
        return {
            id,
            type: "joystick",
            xPort: "B",
            yPort: "C",
            stopAction: "brake",
            multiMotor: false,
            xMotors: [{ port: "B", invert: false }],
            yMotors: [{ port: "C", invert: false }]
        };
    },

    getPortBadge(w) {
        return formatPortBadge(w);
    },

    // Generates configuration markup for single and multi-actuator layouts.
    renderDesign(w, ctx) {
        const motorCols = ["A", "B", "C", "D"];
        const axes = [
            { axis: "x", label: "X Axis (Left / Right)", motors: w.xMotors },
            { axis: "y", label: "Y Axis (Forward / Reverse)", motors: w.yMotors }
        ];

        return `
            <div class="widget-config-field full-width">
                <label class="checkbox-label">
                    <input type="checkbox" class="field-multi-motor" data-id="${w.id}" ${w.multiMotor ? "checked" : ""}>
                    <span>Multi-Motor Control</span>
                </label>
            </div>
            ${!w.multiMotor ? `
                <div class="widget-config-field">
                    <label>X Axis (Left / Right)</label>
                    <select class="field-x-port" data-id="${w.id}">${ctx.portOptions(w.xPort)}</select>
                </div>
                <div class="widget-config-field">
                    <label>Y Axis (Forward / Reverse)</label>
                    <select class="field-y-port" data-id="${w.id}">${ctx.portOptions(w.yPort)}</select>
                </div>
            ` : `
                <div class="multi-motor-container full-width">
                    ${axes.map(({ axis, label, motors }) => `
                        <div class="multi-motor-section">
                            <span class="multi-motor-axis-title">${label}</span>
                            <div class="multi-motor-grid">
                                ${motorCols.map(col => {
                                    const entry = (motors ?? []).find(m => m.port === col);
                                    const isActive = !!entry;
                                    const isInvert = entry?.invert ?? false;
                                    return `
                                        <div class="multi-motor-row">
                                            <span class="multi-motor-port-tag">${col}</span>
                                            <label class="multi-motor-toggle-label">
                                                <input type="checkbox" class="field-mm-active"
                                                       data-id="${w.id}" data-axis="${axis}" data-port="${col}"
                                                       ${isActive ? "checked" : ""}>
                                                <span>Active</span>
                                            </label>
                                            <label class="multi-motor-invert-label ${!isActive ? "disabled" : ""}">
                                                <input type="checkbox" class="field-mm-invert"
                                                       data-id="${w.id}" data-axis="${axis}" data-port="${col}"
                                                       ${isInvert ? "checked" : ""} ${!isActive ? "disabled" : ""}>
                                                <span>Invert</span>
                                            </label>
                                        </div>
                                    `;
                                }).join("")}
                            </div>
                        </div>
                    `).join("")}
                </div>
            `}
            <div class="widget-config-field full-width">
                <label>Release Action</label>
                <select class="field-stop-action" data-id="${w.id}">
                    <option value="brake" ${w.stopAction === "brake" ? "selected" : ""}>Brake (Hold Position)</option>
                    <option value="coast" ${w.stopAction === "coast" ? "selected" : ""}>Coast (Free Spin)</option>
                </select>
            </div>
        `;
    },

    // Attaches listeners for channel assignments and inversion options.
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-stop-action")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { stopAction: e.target.value });
        });

        [".field-x-port", ".field-y-port"].forEach(selector => {
            const axis = selector.includes("x") ? "x" : "y";
            cardElement.querySelector(selector)?.addEventListener("change", (e) => {
                const port = e.target.value;
                const patch = { [`${axis}Port`]: port };
                if (!w.multiMotor) {
                    patch[`${axis}Motors`] = [{ port, invert: false }];
                }
                ctx.updateWidget(w.id, patch);
            });
        });

        cardElement.querySelector(".field-multi-motor")?.addEventListener("change", (e) => {
            const enabled = e.target.checked;
            const patch = { multiMotor: enabled };
            if (enabled) {
                patch.xMotors = w.xMotors?.length ? w.xMotors : [{ port: w.xPort ?? "B", invert: false }];
                patch.yMotors = w.yMotors?.length ? w.yMotors : [{ port: w.yPort ?? "C", invert: false }];
            }
            ctx.updateWidget(w.id, patch);
            ctx.renderDesignWidgets();
        });

        cardElement.querySelectorAll(".field-mm-active").forEach(el => {
            el.addEventListener("change", (e) => {
                const { axis, port } = e.target.dataset;
                const key = axis === "x" ? "xMotors" : "yMotors";
                const list = [...(w[key] ?? [])];
                const idx = list.findIndex(m => m.port === port);
                const isActive = e.target.checked;
                const patch = {};

                if (isActive) {
                    if (idx === -1) list.push({ port, invert: false });
                    const oppKey = axis === "x" ? "yMotors" : "xMotors";
                    const oppList = (w[oppKey] ?? []).filter(m => m.port !== port);
                    patch[oppKey] = oppList;

                    const oppCheckbox = cardElement.querySelector(
                        `.field-mm-active[data-axis="${axis === "x" ? "y" : "x"}"][data-port="${port}"]`
                    );
                    if (oppCheckbox && oppCheckbox.checked) {
                        oppCheckbox.checked = false;
                        const oppRow = oppCheckbox.closest(".multi-motor-row");
                        const oppInvert = oppRow?.querySelector(".field-mm-invert");
                        const oppInvertLabel = oppRow?.querySelector(".multi-motor-invert-label");
                        if (oppInvert) { oppInvert.checked = false; oppInvert.disabled = true; }
                        if (oppInvertLabel) oppInvertLabel.classList.add("disabled");
                    }
                } else {
                    if (idx !== -1) list.splice(idx, 1);
                }

                patch[key] = list;
                if (list.length > 0) {
                    if (axis === "x") patch.xPort = list[0].port;
                    else patch.yPort = list[0].port;
                }
                ctx.updateWidget(w.id, patch);

                const row = el.closest(".multi-motor-row");
                const invertInput = row?.querySelector(".field-mm-invert");
                const invertLabel = row?.querySelector(".multi-motor-invert-label");
                if (invertInput && invertLabel) {
                    invertInput.disabled = !isActive;
                    if (!isActive) invertInput.checked = false;
                    invertLabel.classList.toggle("disabled", !isActive);
                }
            });
        });

        cardElement.querySelectorAll(".field-mm-invert").forEach(el => {
            el.addEventListener("change", (e) => {
                const { axis, port } = e.target.dataset;
                const key = axis === "x" ? "xMotors" : "yMotors";
                const list = (w[key] ?? []).map(m => m.port === port ? { ...m, invert: e.target.checked } : m);
                ctx.updateWidget(w.id, { [key]: list });
            });
        });
    },

    // Generates circular touch pad markup with crosshairs and readouts.
    renderRun(w) {
        return `
            <div class="joystick-container">
                <div class="joystick-boundary" id="joy-boundary-${w.id}">
                    <div class="joystick-crosshair-x"></div>
                    <div class="joystick-crosshair-y"></div>
                    <div class="joystick-knob" id="joy-knob-${w.id}"></div>
                </div>
                <div class="joystick-readout" id="joy-readout-${w.id}">X: 0% | Y: 0%</div>
            </div>
        `;
    },

    // Captures pointer coordinates and dispatches differential vectors.
    initRun(w, cardElement, ctx) {
        const boundary = cardElement.querySelector(`#joy-boundary-${w.id}`);
        const knob = cardElement.querySelector(`#joy-knob-${w.id}`);
        const readout = cardElement.querySelector(`#joy-readout-${w.id}`);
        if (!boundary || !knob) return;

        let dragging = false;
        const maxRadius = Math.max(30, (boundary.clientWidth - knob.clientWidth) / 2 || 70);

        const onPointerMove = (clientX, clientY) => {
            const rect = boundary.getBoundingClientRect();
            const rawX = clientX - (rect.left + rect.width / 2);
            const rawY = clientY - (rect.top + rect.height / 2);
            const dist = Math.hypot(rawX, rawY);
            const angle = Math.atan2(rawY, rawX);
            const clampedDist = Math.min(dist, maxRadius);
            const dx = clampedDist * Math.cos(angle);
            const dy = clampedDist * Math.sin(angle);

            knob.style.transform = `translate(${dx}px, ${dy}px)`;

            const normX = dx / maxRadius;
            const normY = -dy / maxRadius;

            if (readout) {
                readout.textContent = `X: ${Math.round(normX * 100)}% | Y: ${Math.round(normY * 100)}%`;
            }

            ctx.throttle(`joy_${w.id}`, 66, () => {
                const commands = calculateJoystickCommands(w, normX, normY);
                if (commands.length > 0) {
                    ctx.apiBatch(commands).catch(() => {});
                }
            });
        };

        const stopMotion = () => {
            if (!dragging) return;
            dragging = false;
            knob.classList.remove("active");
            knob.style.transform = "translate(0px, 0px)";
            if (readout) readout.textContent = "X: 0% | Y: 0%";

            ctx.clearThrottle(`joy_${w.id}`);

            const commands = getJoystickStopCommands(w);
            if (commands.length > 0) {
                ctx.apiBatch(commands).catch(() => {});
            }
        };

        boundary.addEventListener("pointerdown", (e) => {
            e.preventDefault();
            dragging = true;
            knob.classList.add("active");
            boundary.setPointerCapture(e.pointerId);
            onPointerMove(e.clientX, e.clientY);
        });

        boundary.addEventListener("pointermove", (e) => {
            if (dragging) {
                e.preventDefault();
                onPointerMove(e.clientX, e.clientY);
            }
        });

        boundary.addEventListener("pointerup", stopMotion);
        boundary.addEventListener("pointercancel", stopMotion);
    },

    // Centers visual elements and resets display percentages on safety stop.
    onEmergencyStop(w, cardElement) {
        const knob = cardElement.querySelector(`#joy-knob-${w.id}`);
        if (knob) {
            knob.style.transform = "translate(0px, 0px)";
            knob.classList.remove("active");
        }
        const readout = cardElement.querySelector(`#joy-readout-${w.id}`);
        if (readout) {
            readout.textContent = "X: 0% | Y: 0%";
        }
    }
};
