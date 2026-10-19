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

// Formats a count with singular or plural noun ("1 step" vs "2 steps").
const plural = (n, word) => `${n} ${n === 1 ? word : `${word}s`}`;

// Returns or initializes the session-scoped keyframe state for a widget ID.
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

// Formats the inner HTML controls for Recording mode or Playback mode.
function renderCardControls(w) {
    const st = getState(w.id);
    if (st.mode === "playback") {
        return `
            <div class="run-button-box" id="rec-box-${w.id}">
                <button class="btn btn-primary btn-big-action btn-action-card" id="btn-rec-play-${w.id}">
                    Playback (${plural(st.keyframes.length, "step")})
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

// Updates the status text in the bottom footer of the widget card.
function setFooter(cardElement, text) {
    const footer = cardElement.querySelector(".run-card-footer");
    if (footer) footer.textContent = text;
}

// Sends a batch brake stop command to the specified motor ports.
async function brakePorts(ctx, ports) {
    if (!ports.length) return;
    const commands = ports.map(port => ({ port, command: "stop", stop_action: "brake" }));
    try {
        await ctx.apiBatch(commands);
    } catch (_) {}
}

// Flags active playback to abort and immediately brakes recorded motor ports.
async function abortPlayback(st, ctx) {
    st.abort = true;
    await brakePorts(ctx, Object.keys(st.keyframes[0] ?? {}));
}

// Drives all motors in a keyframe to their target positions and waits for completion.
async function moveToKeyframe(st, frame, ctx) {
    const getMotor = (port) => ctx.hardwareMotors?.find(m => m.port === port && m.connected);

    // Builds run-to-rel-pos commands for ports whose remaining error exceeds minDelta.
    const buildCommands = (ports, minDelta) => ports.flatMap(port => {
        const m = getMotor(port);
        const delta = m ? frame[port].position - m.position : 0;
        return Math.abs(delta) > minDelta ? [{
            port,
            command: "run-to-rel-pos",
            speed: m.max_speed || frame[port].maxSpeed || 1050,
            degrees: delta,
            stop_action: "brake"
        }] : [];
    });

    const commands = buildCommands(Object.keys(frame), 3);
    if (commands.length === 0) return;
    const activePorts = commands.map(c => c.port);

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
            const err = Math.abs(frame[port].position - m.position);
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

        // Refreshes in-flight moves every 3.5 s so the 5 s watchdog does not coast.
        if (now - lastKeepalive >= 3500) {
            lastKeepalive = now;
            const movingPorts = activePorts.filter(p => !progress.get(p)?.stalled);
            const refreshCmds = buildCommands(movingPorts, 10);
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

    // Requires only id and type because all connected motors are recorded.
    createDefault(id) {
        return { id, type: "record" };
    },

    // Indicates that the recorder captures all connected motor ports.
    getPortBadge() {
        return "All Motors";
    },

    // Renders a short description in Design View.
    renderDesign() {
        return `
            <div class="widget-config-field full-width">
                <span class="text-muted">Records and plays back positions for all connected motors (A–D).</span>
            </div>
        `;
    },

    // Delegates Run View markup to the active Recording or Playback mode.
    renderRun(w) {
        return renderCardControls(w);
    },

    // Binds Record, Done, Playback, and Re-record buttons for the active mode.
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
            setFooter(cardElement, count === 0 ? "Ready to record" : `${plural(count, "keyframe")} recorded`);

            const btnAdd = cardElement.querySelector(`#btn-rec-add-${w.id}`);
            const btnDone = cardElement.querySelector(`#btn-rec-done-${w.id}`);

            btnAdd?.addEventListener("click", () => {
                const connected = (ctx.hardwareMotors ?? []).filter(m => m.connected);
                if (connected.length === 0) {
                    setFooter(cardElement, "No connected motors found");
                    return;
                }
                const snapshot = Object.fromEntries(
                    connected.map(m => [m.port, { position: m.position, maxSpeed: m.max_speed || 1050 }])
                );
                st.keyframes.push(snapshot);
                setFooter(cardElement, `${plural(st.keyframes.length, "keyframe")} recorded`);
                if (btnDone) btnDone.disabled = false;
            });

            btnDone?.addEventListener("click", () => {
                if (st.keyframes.length === 0) return;
                st.mode = "playback";
                refreshCard();
                setFooter(cardElement, `Ready (${plural(st.keyframes.length, "step")})`);
            });
            return;
        }

        const btnPlay = cardElement.querySelector(`#btn-rec-play-${w.id}`);
        const btnReset = cardElement.querySelector(`#btn-rec-reset-${w.id}`);

        const updatePlayButton = () => {
            if (!btnPlay) return;
            btnPlay.classList.toggle("btn-danger", st.playing);
            btnPlay.classList.toggle("btn-primary", !st.playing);
            if (!st.playing) {
                btnPlay.textContent = `Playback (${plural(st.keyframes.length, "step")})`;
            }
        };

        btnPlay?.addEventListener("click", async () => {
            if (st.playing) {
                await abortPlayback(st, ctx);
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
                setFooter(cardElement, st.abort ? "Playback stopped" : `Playback complete (${plural(total, "step")})`);
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
            if (st.playing) await abortPlayback(st, ctx);
            st.keyframes = [];
            st.mode = "record";
            refreshCard();
        });
    },

    // Aborts active keyframe playback when global emergency stop is triggered.
    onEmergencyStop(w, cardElement) {
        const st = stateMap.get(w.id);
        if (st && st.playing) {
            st.abort = true;
            setFooter(cardElement, "Emergency stopped");
        }
    }
};
