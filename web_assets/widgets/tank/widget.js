export default {
    type: "tank",
    label: "Tank Controls",
    sizeClass: "run-card-2x2",

    // Seeds initial layout schema when adding dual-track controls.
    createDefault(id) {
        return {
            id,
            type: "tank",
            leftPort: "B",
            rightPort: "C",
            stopAction: "brake"
        };
    },

    getPortBadge(w) {
        return `L: ${w.leftPort} | R: ${w.rightPort}`;
    },

    // Generates configuration markup for left and right actuator tracks.
    renderDesign(w, ctx) {
        return `
            <div class="widget-config-field">
                <label>Left Track</label>
                <select class="field-left-port" data-id="${w.id}">${ctx.portOptions(w.leftPort)}</select>
            </div>
            <div class="widget-config-field">
                <label>Right Track</label>
                <select class="field-right-port" data-id="${w.id}">${ctx.portOptions(w.rightPort)}</select>
            </div>
            <div class="widget-config-field full-width">
                <label>Release Action</label>
                <select class="field-stop-action" data-id="${w.id}">
                    <option value="brake" ${w.stopAction === "brake" ? "selected" : ""}>Brake (Hold Position)</option>
                    <option value="coast" ${w.stopAction === "coast" ? "selected" : ""}>Coast (Free Spin)</option>
                </select>
            </div>
        `;
    },

    // Attaches listeners for channel assignments and braking options.
    attachDesign(w, cardElement, ctx) {
        cardElement.querySelector(".field-left-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { leftPort: e.target.value });
        });
        cardElement.querySelector(".field-right-port")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { rightPort: e.target.value });
        });
        cardElement.querySelector(".field-stop-action")?.addEventListener("change", (e) => {
            ctx.updateWidget(w.id, { stopAction: e.target.value });
        });
    },

    // Generates dual vertical track sliders with velocity displays.
    renderRun(w) {
        return `
            <div class="tank-controls-container">
                <div class="tank-track-col">
                    <span class="tank-track-title">Left (${w.leftPort})</span>
                    <div class="tank-slider-wrapper">
                        <input type="range" class="run-tank-slider" id="tank-left-${w.id}"
                               min="-1000" max="1000" value="0" step="25" data-id="${w.id}" data-side="left"
                               orient="vertical">
                    </div>
                    <span class="tank-speed-readout" id="tank-left-val-${w.id}">0</span>
                </div>
                <div class="tank-track-divider"></div>
                <div class="tank-track-col">
                    <span class="tank-track-title">Right (${w.rightPort})</span>
                    <div class="tank-slider-wrapper">
                        <input type="range" class="run-tank-slider" id="tank-right-${w.id}"
                               min="-1000" max="1000" value="0" step="25" data-id="${w.id}" data-side="right"
                               orient="vertical">
                    </div>
                    <span class="tank-speed-readout" id="tank-right-val-${w.id}">0</span>
                </div>
            </div>
            <div class="tank-actions">
                <button class="btn btn-sm btn-secondary btn-tank-zero" data-id="${w.id}">Stop Both</button>
            </div>
        `;
    },

    // Maps vertical slider offsets to throttled dual-channel batch vectors.
    initRun(w, cardElement, ctx) {
        const leftSlider = cardElement.querySelector(`#tank-left-${w.id}`);
        const rightSlider = cardElement.querySelector(`#tank-right-${w.id}`);
        const leftVal = cardElement.querySelector(`#tank-left-val-${w.id}`);
        const rightVal = cardElement.querySelector(`#tank-right-val-${w.id}`);
        const btnZero = cardElement.querySelector(`.btn-tank-zero[data-id="${w.id}"]`);
        if (!leftSlider || !rightSlider) return;

        const updateSpeeds = () => {
            const leftSpeed = parseInt(leftSlider.value, 10);
            const rightSpeed = parseInt(rightSlider.value, 10);
            if (leftVal) leftVal.textContent = leftSpeed;
            if (rightVal) rightVal.textContent = rightSpeed;

            ctx.throttle(`tank_${w.id}`, 66, () => {
                const stopAction = w.stopAction || "brake";
                const commands = [
                    { port: w.leftPort, speed: leftSpeed, stop_action: stopAction },
                    { port: w.rightPort, speed: rightSpeed, stop_action: stopAction }
                ];
                ctx.apiBatch(commands).catch(() => {});
            });
        };

        leftSlider.addEventListener("input", updateSpeeds);
        rightSlider.addEventListener("input", updateSpeeds);

        btnZero?.addEventListener("click", () => {
            leftSlider.value = "0";
            rightSlider.value = "0";
            if (leftVal) leftVal.textContent = "0";
            if (rightVal) rightVal.textContent = "0";
            ctx.clearThrottle(`tank_${w.id}`);
            const stopAction = w.stopAction || "brake";
            ctx.apiBatch([
                { port: w.leftPort, speed: 0, stop_action: stopAction },
                { port: w.rightPort, speed: 0, stop_action: stopAction }
            ]).catch(() => {});
        });
    },

    // Clears throttle and resets visual sliders to zero on safety stop.
    onEmergencyStop(w, cardElement) {
        const leftSlider = cardElement.querySelector(`#tank-left-${w.id}`);
        const rightSlider = cardElement.querySelector(`#tank-right-${w.id}`);
        const leftVal = cardElement.querySelector(`#tank-left-val-${w.id}`);
        const rightVal = cardElement.querySelector(`#tank-right-val-${w.id}`);
        if (leftSlider) leftSlider.value = "0";
        if (rightSlider) rightSlider.value = "0";
        if (leftVal) leftVal.textContent = "0";
        if (rightVal) rightVal.textContent = "0";
    }
};
