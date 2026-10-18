/**
 * # Keyframe Recorder Widget Module
 *
 * This modular widget records motor encoder positions across all connected motors (A..D)
 * as a list of keyframes in memory and plays them back sequentially at maximum speed.
 *
 * Implements the modular widget interface (`createDefault`, `getPortBadge`, `renderDesign`,
 * `renderRun`, `initRun`, `onEmergencyStop`). Dynamically discovered and loaded from `/widgets/record.js`.
 */

const stateMap = new Map();

/**
 * Returns or initializes the in-memory recording and playback state for a widget ID.
 *
 * Keeps recorded keyframes in browser memory for the current session without persisting to localStorage.
 */
function getState(id) {
    if (!stateMap.has(id)) {
        stateMap.set(id, {
            keyframes: [],
            mode: "record",
            playing: false,
            abort: false
        });
    }
    return stateMap.get(id);
}

/**
 * Formats the inner HTML controls of the card for either Recording mode or Playback mode.
 *
 * Separated so switching between Recording and Playback updates only this widget card.
 */
function renderCardControls(w) {
    const st = getState(w.id);
    if (st.mode === "playback") {
        const count = st.keyframes.length;
        return `
            <div class="run-button-box" id="rec-box-${w.id}">
                <button class="btn btn-primary btn-big-action btn-action-card" id="btn-rec-play-${w.id}">
                    Playback (${count} ${count === 1 ? "step" : "steps"})
                </button>
                <div class="slider-actions" style="margin-top: 8px;">
                    <button class="btn btn-sm btn-secondary" id="btn-rec-reset-${w.id}">Re-record</button>
                </div>
            </div>
        `;
    }

    const hasFrames = st.keyframes.length > 0;
    return `
        <div class="run-button-box" id="rec-box-${w.id}">
            <button class="btn btn-primary btn-big-action btn-action-card" id="btn-rec-add-${w.id}">
                Record Keyframe
            </button>
            <div class="slider-actions" style="margin-top: 8px;">
                <button class="btn btn-sm btn-secondary" id="btn-rec-done-${w.id}" ${hasFrames ? "" : "disabled"}>
                    Done Recording
                </button>
            </div>
        </div>
    `;
}

/**
 * Updates the status text in the bottom footer of the widget card.
 *
 * Centralizes footer status updates during recording and keyframe playback.
 */
function setFooter(cardElement, text) {
    const footer = cardElement.querySelector(".run-card-footer");
    if (footer) footer.textContent = text;
}

/**
 * Stops all recorded motor ports with brake action.
 *
 * Used when cancelling playback or releasing a stalled motor before advancing to the next keyframe.
 */
async function brakePorts(ctx, ports) {
    if (!ports.length) return;
    const commands = ports.map(port => ({ port, command: "stop", stop_action: "brake" }));
    try {
        await ctx.apiBatch(commands);
    } catch (_) {}
}

/**
 * Drives all motors in `frame` to their recorded encoder positions at maximum speed and waits until complete.
 *
 * Monitors live motor telemetry every 100 ms:
 * - Marks a motor as at-goal when within 10 degrees of its target position.
 * - Detects stalled motors when `state` reports "stalled"/"overloaded", when the kernel finishes
 *   `run-to-rel-pos` (`!state.includes("running")`), or when position changes by less than 3 degrees
 *   across 500 ms.
 * - Once all non-stalled motors reach their goal, waits at most 1 second (1000 ms) if any motor is stalled,
 *   stops the stalled motor so it does not keep straining, and resolves to advance to the next keyframe.
 */
async function moveToKeyframe(st, frame, ctx) {
    const ports = Object.keys(frame);
    const getMotor = (port) => ctx.hardwareMotors?.find(m => m.port === port && m.connected);

    const commands = [];
    const activePorts = [];

    for (const port of ports) {
        const m = getMotor(port);
        if (!m) continue;
        const target = frame[port].position;
        const delta = target - m.position;
        if (Math.abs(delta) > 3) {
            commands.push({
                port,
                command: "run-to-rel-pos",
                speed: m.max_speed || frame[port].maxSpeed || 1050,
                degrees: delta,
                stop_action: "brake"
            });
            activePorts.push(port);
        }
    }

    if (commands.length === 0) return;

    await ctx.apiBatch(commands);

    const startTime = Date.now();
    let lastKeepalive = startTime;
    let stallDeadline = null;
    const progress = new Map(
        activePorts.map(p => [p, { lastPos: getMotor(p)?.position ?? 0, lastCheck: startTime, stalled: false }])
    );

    while (!st.abort) {
        await new Promise(r => setTimeout(r, 100));
        if (st.abort) break;

        const now = Date.now();
        const elapsed = now - startTime;
        let allAtGoal = true;
        let allNonStalledDone = true;
        let anyStalled = false;

        for (const port of activePorts) {
            const m = getMotor(port);
            if (!m) continue;
            const target = frame[port].position;
            const err = Math.abs(target - m.position);
            const tr = progress.get(port);

            if (err <= 10) {
                tr.stalled = false;
                continue;
            }

            allAtGoal = false;

            const isRunning = m.state?.includes("running");
            const hwStalled = m.state?.includes("stalled") || m.state?.includes("overloaded");

            if (now - tr.lastCheck >= 500) {
                const moved = Math.abs(m.position - tr.lastPos);
                tr.lastPos = m.position;
                tr.lastCheck = now;
                if (elapsed >= 350 && moved < 3) {
                    tr.stalled = true;
                }
            }

            if (elapsed >= 300 && (!isRunning || hwStalled)) {
                tr.stalled = true;
            }

            if (tr.stalled) {
                anyStalled = true;
            } else {
                allNonStalledDone = false;
            }
        }

        if (allAtGoal) break;

        if (allNonStalledDone && anyStalled) {
            if (stallDeadline === null) {
                stallDeadline = now + 1000;
            } else if (now >= stallDeadline) {
                const stalledPorts = activePorts.filter(p => progress.get(p)?.stalled);
                await brakePorts(ctx, stalledPorts);
                break;
            }
        } else {
            stallDeadline = null;
        }

        // Refresh remaining relative movement every 3.5s during long moves so the 5s idle watchdog does not coast
        if (now - lastKeepalive >= 3500) {
            lastKeepalive = now;
            const refreshCmds = [];
            for (const port of activePorts) {
                const m = getMotor(port);
                if (!m || progress.get(port)?.stalled) continue;
                const delta = frame[port].position - m.position;
                if (Math.abs(delta) > 10) {
                    refreshCmds.push({
                        port,
                        command: "run-to-rel-pos",
                        speed: m.max_speed || frame[port].maxSpeed || 1050,
                        degrees: delta,
                        stop_action: "brake"
                    });
                }
            }
            if (refreshCmds.length > 0) {
                ctx.apiBatch(refreshCmds).catch(() => {});
            }
        }
    }
}

export default {
    type: "record",
    label: "Keyframe Recorder",
    sizeClass: "run-card-2x1",

    /**
     * Creates default configuration object when adding a Keyframe Recorder widget.
     *
     * Requires no port configuration because it automatically records all connected motors.
     */
    createDefault(id) {
        return { id, type: "record" };
    },

    /**
     * Formats the badge text in the card header.
     *
     * Indicates that the recorder operates across all connected motors.
     */
    getPortBadge() {
        return "All Motors";
    },

    /**
     * Renders the Design View card body.
     *
     * Displays a concise note explaining that all connected motors are recorded automatically.
     */
    renderDesign() {
        return `
            <div class="widget-config-field full-width">
                <span class="text-muted">Records and plays back positions for all connected motors (A–D).</span>
            </div>
        `;
    },

    /**
     * Renders the Run View card body.
     *
     * Delegates to `renderCardControls` based on whether the widget is in Recording or Playback mode.
     */
    renderRun(w) {
        return renderCardControls(w);
    },

    /**
     * Wires button event handlers for recording keyframes, finishing recording, playing back, and re-recording.
     *
     * Re-binds cleanly whenever the card transitions between Recording and Playback states.
     */
    initRun(w, cardElement, ctx) {
        const st = getState(w.id);

        const refreshCard = () => {
            const box = cardElement.querySelector(`#rec-box-${w.id}`);
            if (box) {
                box.outerHTML = renderCardControls(w);
            }
            this.initRun(w, cardElement, ctx);
        };

        if (st.mode === "record") {
            const count = st.keyframes.length;
            setFooter(cardElement, count === 0 ? "Ready to record" : `${count} ${count === 1 ? "keyframe" : "keyframes"} recorded`);

            const btnAdd = cardElement.querySelector(`#btn-rec-add-${w.id}`);
            const btnDone = cardElement.querySelector(`#btn-rec-done-${w.id}`);

            btnAdd?.addEventListener("click", () => {
                const connected = (ctx.hardwareMotors ?? []).filter(m => m.connected);
                if (connected.length === 0) {
                    setFooter(cardElement, "No connected motors found");
                    return;
                }
                const snapshot = {};
                connected.forEach(m => {
                    snapshot[m.port] = {
                        position: m.position,
                        maxSpeed: m.max_speed || 1050
                    };
                });
                st.keyframes.push(snapshot);
                const total = st.keyframes.length;
                setFooter(cardElement, `${total} ${total === 1 ? "keyframe" : "keyframes"} recorded`);
                if (btnDone) btnDone.disabled = false;
            });

            btnDone?.addEventListener("click", () => {
                if (st.keyframes.length === 0) return;
                st.mode = "playback";
                refreshCard();
                setFooter(cardElement, `Ready (${st.keyframes.length} steps)`);
            });
            return;
        }

        // Playback mode
        const btnPlay = cardElement.querySelector(`#btn-rec-play-${w.id}`);
        const btnReset = cardElement.querySelector(`#btn-rec-reset-${w.id}`);

        const updatePlayButton = () => {
            if (!btnPlay) return;
            const total = st.keyframes.length;
            if (st.playing) {
                btnPlay.classList.remove("btn-primary");
                btnPlay.classList.add("btn-danger");
            } else {
                btnPlay.classList.remove("btn-danger");
                btnPlay.classList.add("btn-primary");
                btnPlay.textContent = `Playback (${total} ${total === 1 ? "step" : "steps"})`;
            }
        };

        btnPlay?.addEventListener("click", async () => {
            if (st.playing) {
                st.abort = true;
                const ports = Object.keys(st.keyframes[0] ?? {});
                await brakePorts(ctx, ports);
                return;
            }

            if (st.keyframes.length === 0) return;
            st.playing = true;
            st.abort = false;
            if (btnReset) btnReset.disabled = true;
            updatePlayButton();

            const total = st.keyframes.length;
            try {
                for (let i = 0; i < total; i++) {
                    if (st.abort) break;
                    if (btnPlay) btnPlay.textContent = `Stop (Step ${i + 1} / ${total})`;
                    setFooter(cardElement, `Moving to keyframe ${i} (${i + 1} / ${total})...`);
                    await moveToKeyframe(st, st.keyframes[i], ctx);
                }
                setFooter(cardElement, st.abort ? "Playback stopped" : `Playback complete (${total} steps)`);
            } catch (e) {
                setFooter(cardElement, `Playback error: ${e.message}`);
            } finally {
                st.playing = false;
                st.abort = false;
                if (btnReset) btnReset.disabled = false;
                updatePlayButton();
            }
        });

        btnReset?.addEventListener("click", async () => {
            if (st.playing) {
                st.abort = true;
                const ports = Object.keys(st.keyframes[0] ?? {});
                await brakePorts(ctx, ports);
            }
            st.keyframes = [];
            st.mode = "record";
            refreshCard();
        });
    },

    /**
     * Aborts active keyframe playback immediately when global Emergency Stop is triggered.
     *
     * Sets `st.abort = true` so the playback loop exits and resets the Playback button.
     */
    onEmergencyStop(w, cardElement) {
        const st = stateMap.get(w.id);
        if (st && st.playing) {
            st.abort = true;
            setFooter(cardElement, "Emergency stopped");
        }
    }
};
