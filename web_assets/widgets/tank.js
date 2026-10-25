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
    { side: "left", label: "Left", key: "leftPort", invKey: "leftInvert" },
    { side: "right", label: "Right", key: "rightPort", invKey: "rightInvert" }
];

// Formats a synchronized batch payload for both tracks from normalized [-1, 1] inputs.
const tankCommands = (w, norms, ctx) =>
    SIDES.map(({ key, invKey }, i) => ({
        port: w[key],
        speed: ctx.scaleSpeed(w[key], norms[i], w[invKey]),
        stop_action: w.stopAction || "brake"
    }));

// Resolves the range input and readout elements for both tracks on a card.
const trackElements = (w, cardElement) => SIDES.map(({ side }) => ({
    slider: cardElement.querySelector(`#tank-${side}-${w.id}`),
    readout: cardElement.querySelector(`#tank-${side}-val-${w.id}`)
}));

// Resets both track sliders and their numeric labels to zero.
function zeroTracks(tracks) {
    tracks.forEach(({ slider, readout }) => {
        if (slider) slider.value = "0";
        if (readout) readout.textContent = "0%";
    });
}

export default {
    type: "tank",
    label: "Tank Controls",
    sizeClass: "run-card-2x2",

    // Assigns ports B and C with normal polarity and brake release for newly added tank cards.
    createDefault(id) {
        return {
            id,
            type: "tank",
            leftPort: "B",
            rightPort: "C",
            leftInvert: false,
            rightInvert: false,
            stopAction: "brake"
        };
    },

    // Formats the left/right track port badge for the card header.
    getPortBadge(w) {
        const fmt = ({ key, invKey }) => `${w[invKey] ? "-" : ""}${w[key]}`;
        return `L: ${fmt(SIDES[0])} | R: ${fmt(SIDES[1])}`;
    },

    // Renders left track, right track, direction invert checkboxes, and release action dropdown.
    renderDesign(w, ctx) {
        return SIDES.map(({ side, label, key, invKey }) =>
            ctx.motorField(`${label} Track`, `field-${side}-port`, w[key], `field-${side}-invert`, w[invKey])
        ).join("") + ctx.stopActionField(w);
    },

    // Binds track ports, direction invert checkboxes, and release action dropdown to widget state.
    attachDesign(w, cardElement, ctx) {
        ctx.bindFields(cardElement, w, {
            ...Object.fromEntries(SIDES.flatMap(({ side, key, invKey }) => [
                [`.field-${side}-port`, key],
                [`.field-${side}-invert`, invKey]
            ])),
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
                           min="-100" max="100" value="0" step="1" orient="vertical">
                </div>
                <span class="tank-speed-readout" id="tank-${side}-val-${w.id}">0%</span>
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

    // Streams 10 Hz dual-track batch commands on slider input or Stop Both.
    initRun(w, cardElement, ctx) {
        const tracks = trackElements(w, cardElement);
        if (tracks.some(t => !t.slider)) return;
        const key = `tank_${w.id}`;
        const send = norms => ctx.apiBatch(tankCommands(w, norms, ctx)).catch(() => {});

        const updateSpeeds = () => {
            const norms = tracks.map(({ slider, readout }, i) => {
                const norm = parseInt(slider.value, 10) / 100;
                const pct = ctx.scalePct(norm, w[SIDES[i].invKey]);
                if (readout) readout.textContent = `${pct}%`;
                return norm;
            });
            ctx.throttle(key, 100, () => send(norms));
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
