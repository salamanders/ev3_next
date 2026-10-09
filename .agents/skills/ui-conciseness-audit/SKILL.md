---
name: ui-conciseness-audit
description: >-
  Audits user interfaces, HTML templates, CSS, and frontend copy to eliminate
  LLM boilerplate, repeated words, redundant labels, unnecessary 'mode' tags,
  vibe-coded clichés, and wasted screen space. Use whenever designing, creating,
  or refactoring UI components, dashboards, forms, or controls.
---

# UI Conciseness and Anti-Slop Audit

This skill guides agents in eliminating verbose boilerplate, repetitive noun echoes, redundant labels, vibe-coded clichés, and layout bloat in frontend interfaces.

## 1. Core Principles

1. **Avoid Repetition:** An LLM writes repetitive text and code easily. Repetition increases maintenance cost and failure points.
2. **Avoid Generating Extra Features, Modes, and Toggles:** An LLM proactively builds extra modes, toggles, buttons, or widgets because it does not get tired. This creates visual and cognitive overload for users.
3. **Always Reduce Word Count:** LLMs generate unbounded output tokens, but human working memory is limited. Fewer words provide clearer communication.
4. **Simplify Language (Simplified Technical English):** Use direct, unambiguous vocabulary to remove fluff and improve comprehension.
5. **Aggressively Remove Adjectives:** Adjectives frequently add noise without information.
   - ❌ `Target Motor` ➔ ✅ `Motor`
   - ❌ `Important Actions to Complete` ➔ ✅ `Actions` or `TODOs`
   - ❌ `Active Connected Devices` ➔ ✅ `Connected Devices`

---

## 2. Interface Anti-Slop Heuristics

### Rule A: The Zero-Filler and Zero-"Mode" Principle
- Words such as "Mode", "State", "Item", "Value", "Entity", "Object", and "System" are almost always redundant metadata.
  - ❌ `Design Mode` / `Run Mode` ➔ ✅ `Design` / `Run`
  - ❌ `Drive Mode: Differential` ➔ ✅ `Steering: Differential`
  - ❌ `Sensor Mode: US-DIST-CM` ➔ ✅ `Reading: US-DIST-CM` (or directly show `US-DIST-CM`)
  - ❌ `Status: Ready` ➔ ✅ `Ready`

### Rule B: Eliminate Noun-Echo (Parent-Child Redundancy)
- When a control is inside a labeled card or container, child inputs must never repeat the parent noun:
  - In a Widget card: ❌ `Widget Title` ➔ ✅ `Title`
  - In a Motor card: ❌ `Target Motor` ➔ ✅ `Motor`
  - In a Sensor card: ❌ `Input Port` ➔ ✅ `Port`
  - In an Axis group: ❌ `Invert X Axis` label above `[ ] Invert X` checkbox ➔ ✅ Checkbox `[ ] Invert X` only

### Rule C: Eliminate Action-Echo (Adjacent Label Duplication)
- Never place a label beside a button or input that repeats the same noun or verb:
  - ❌ `Add Widget: [ Add Widget ]` ➔ ✅ `Type: [ Add ]`
  - ❌ `Hardware: [ Rescan Hardware ]` ➔ ✅ `[ Rescan ]` (when inside or above Hardware panel)
  - ❌ `Command Log: [ Clear Log ]` ➔ ✅ `Command Log: [ Clear ]`
  - ❌ Empty state text: `"Open Design to configure..."` above button `[ Open Design ]` ➔ ✅ `"Configure controls..."` above `[ Open Design ]`

### Rule D: Eliminate Category Suffixes in Select Options
- In dropdown menus where the category is already established, do not repeat the category name in every choice:
  - ❌ `Momentary Button`, `Toggle Button`, `Timed Move Button`, `Step Angle Button`
  - ✅ `Momentary (Hold to Run)`, `Toggle (Run/Stop)`, `Timed Move`, `Step Angle`

### Rule E: Vertical Compression and Screen Real Estate
- **Top Bar:** Reserve top navigation exclusively for core views and tabs.
- **Footer Status Bar:** Move passive telemetry (`Battery`, `RTT`, connection status) and emergency controls (`STOP ALL`, `Power Off`) to an unobtrusive footer.
- **Above-the-Fold Priority:** Ensure primary action controls (such as 2D joysticks and sliders) are visible without scrolling on mobile and desktop viewports.

---

## 3. Anti-Vibe-Coding and Anti-Cliché Rules

### Rule F: No Emoji-First Bullet Points
- Never prefix headers, subheaders, or list items with literal emojis.
  - ❌ `✨ Transform Workflow`, `🚀 Launch Server`, `💡 Quick Tip`, `🛠️ Port Tools`, `🎯 Active Goal`
  - ✅ `Transform Workflow`, `Launch Server`, `Tip`, `Port Tools`, `Active Goal`

### Rule G: Eliminate Marketing and AI Buzzwords
- Remove formulaic corporate buzzwords from interface copy:
  - ❌ Forbidden words: `Revolutionize`, `Unlock`, `Seamless`, `Elevate`, `Empower`, `Navigate`, `Transform`, `Supercharge`
  - ✅ Use factual, descriptive technical terms: `Configure`, `Start`, `Stop`, `Connect`, `Set`.

### Rule H: Eliminate Phantom Feature Buttons
- Never render non-functional placeholder buttons (for example, decorative "Export to CSV", "Share", or buttons that trigger "Coming soon!" alerts).
- Only render controls that have implemented, verified backend functionality.

### Rule I: Eliminate Visual Clichés
- Avoid generic template styling tropes:
  - No pulsing electric blue or purple neon border lines.
  - No blurry background gradient orbs (violet-to-indigo radial blobs).
  - No bento-box layouts that waste screen space for one icon and one sentence.
  - Use clean, functional design with high-contrast text and purposeful spacing.

### Rule J: Native Browser Standards and Accessibility
- Use semantic HTML tags (`<label>`, `<input>`, `<select>`, `<button>`).
- Maintain valid `for` / `id` label associations.
- Never use custom pointer-event wrappers that break keyboard Tab navigation, native scrolling, or browser password managers.
- When implementing touch controls (such as virtual joysticks), explicitly use `touch-action: none` to isolate touch gestures from page scrolling.

---

## 4. Step-by-Step Audit Procedure

Whenever generating or editing UI templates:

1. **Run Automated Copy Audit:**
   Execute the verification script to check for forbidden filler words, emojis, and buzzwords:
   ```bash
   python3 .agents/skills/ui-conciseness-audit/scripts/audit_ui_copy.py
   ```

2. **Inspect Visible Frequency Counts:**
   Scan rendered HTML text to identify repeated words across contiguous components:
   ```bash
   grep -o -E '\b[A-Za-z]+\b' web_assets/*.html | tr '[:upper:]' '[:lower:]' | sort | uniq -c | sort -nr | head -n 30
   ```

3. **Verify Zero Forbidden Filler Words:**
   Confirm that generic filler words (such as "Mode") appear 0 times in visible text:
   ```bash
   grep -in "mode" web_assets/*.html web_assets/*.js
   ```

4. **Visual Inspection:**
   Render the UI in headless browser simulation, capture desktop and mobile viewports, and visually verify:
   - Above-the-fold layout density.
   - Absence of duplicate adjacent labels.
   - Clean footer status bar placement.
