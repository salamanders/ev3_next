/**
 * # Tank Drive Controls Widget Module
 *
 * This modular widget renders twin vertical sliders for differential tank-steering robots.
 * It provides independent speed control for left and right drive tracks with a shared "Stop Both" button.
 *
 * Implements the modular widget interface (`createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`).
 * Dynamically loaded by `WidgetRegistry` on demand from `/widgets/tank.js`.
 */

// Track definitions shared across design inputs, run columns, and commands.
const SIDES = [
    { side: "left", label: "Left", key: "leftPort" },
    { side: "right", label: "Right", key: "rightPort" }
];

// Formats a synchronized batch payload for both tracks from a speed pair.
const tankCommands = (w, speeds) =>
    SIDES.map(({ key }, i) => ({ port: w[key], speed: speeds[i], stop_action: w.stopAction || "brake" }));

// Resolves the range input and readout elements for both tracks on a card.
const trackElements = (w, cardElement) => SIDES.map(({ side }) => ({
    slider: cardElement.querySelector(`#tank-${side}-${w.id}`),
    readout: cardElement.querySelector(`#tank-${side}-val-${w.id}`)
}));

// Resets both track sliders and their numeric labels to zero.
function zeroTracks(tracks) {
    tracks.forEach(({ slider, readout }) => {
        if (slider) slider.value = "0";
        if (readout) readout.textContent = "0";
    });
}

export default {
    type: "tank",
    label: "Tank Controls",
    sizeClass: "run-card-2x2",

    // Assigns ports B and C with brake action for newly added tank cards.
    createDefault(id) {
        return {
            id,
            type: "tank",
            leftPort: "B",
            rightPort: "C",
            stopAction: "brake"
        };
    },

    // Formats the left/right track port badge for the card header.
    getPortBadge(w) {
        return `L: ${w.leftPort} | R: ${w.rightPort}`;
    },

    // Renders left track, right track, and release action dropdowns.
    renderDesign(w, ctx) {
        return SIDES.map(({ side, label, key }) => ctx.portField(`${label} Track`, `field-${side}-port`, w[key])).join("")
            + ctx.stopActionField(w);
    },

    // Binds track port and release action dropdowns to widget state.
    attachDesign(w, cardElement, ctx) {
        ctx.bindFields(cardElement, w, {
            ...Object.fromEntries(SIDES.map(({ side, key }) => [`.field-${side}-port`, key])),
            ".field-stop-action": "stopAction"
        });
    },

    // Renders dual vertical track sliders and the shared Stop Both button.
    renderRun(w) {
        const columns = SIDES.map(({ side, label, key }) => `
            <div class="tank-track-col">
                <span class="tank-track-title">${label} (${w[key]})</span>
                <div class="tank-slider-wrapper">
                    <input type="range" class="run-tank-slider" id="tank-${side}-${w.id}"
                           min="-1000" max="1000" value="0" step="25" orient="vertical">
                </div>
                <span class="tank-speed-readout" id="tank-${side}-val-${w.id}">0</span>
            </div>
        `);
        return `
            <div class="tank-controls-container">
                ${columns.join(`<div class="tank-track-divider"></div>`)}
            </div>
            <div class="tank-actions">
                <button class="btn btn-sm btn-secondary btn-tank-zero">Stop Both</button>
            </div>
        `;
    },

    // Streams 15 Hz dual-track batch commands on slider input or Stop Both.
    initRun(w, cardElement, ctx) {
        const tracks = trackElements(w, cardElement);
        if (tracks.some(t => !t.slider)) return;
        const key = `tank_${w.id}`;
        const send = speeds => ctx.apiBatch(tankCommands(w, speeds)).catch(() => {});

        const updateSpeeds = () => {
            const speeds = tracks.map(({ slider, readout }) => {
                const speed = parseInt(slider.value, 10);
                if (readout) readout.textContent = speed;
                return speed;
            });
            ctx.throttle(key, 66, () => send(speeds));
        };

        tracks.forEach(({ slider }) => slider.addEventListener("input", updateSpeeds));

        cardElement.querySelector(".btn-tank-zero")?.addEventListener("click", () => {
            zeroTracks(tracks);
            ctx.clearThrottle(key);
            send([0, 0]);
        });
    },

    // Zeroes both vertical track sliders when emergency stop is triggered.
    onEmergencyStop(w, cardElement) {
        zeroTracks(trackElements(w, cardElement));
    }
};
