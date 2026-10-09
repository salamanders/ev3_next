---
name: ui-conciseness-audit
description: >-
  Audits user interfaces, HTML templates, CSS, and frontend copy to eliminate
  LLM boilerplate, repeated words, redundant labels, unnecessary 'mode' tags,
  and wasted screen space. Use whenever designing, creating, or refactoring
  UI components, dashboards, forms, or controls.
---

# UI Conciseness and Anti-Slop Audit

This skill guides agents in eliminating verbose boilerplate, repetitive noun echoes, redundant labels, and layout bloat in frontend interfaces.

## 1. Core Heuristics: The Anti-Slop Rules

### Rule A: The Zero-Filler and Zero-"Mode" Principle
- Eliminate filler words: Words such as "Mode", "State", "Item", "Value", "Entity", "Object", and "System" are often redundant metadata.
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
- Never place a label beside a button or input that states the exact same noun or verb:
  - ❌ `Add Widget: [ Add Widget ]` ➔ ✅ `Type: [ Add ]`
  - ❌ `Hardware: [ Rescan Hardware ]` ➔ ✅ `[ Rescan ]` (when inside or above Hardware panel)
  - ❌ `Command Log: [ Clear Log ]` ➔ ✅ `Command Log: [ Clear ]`
  - ❌ Empty state text: `"Open Design to configure..."` above button `[ Open Design ]` ➔ ✅ `"Configure controls..."` above `[ Open Design ]`

### Rule D: Eliminate Category Suffixes in Select Options
- In dropdowns where the category is already established, do not repeat the category name in every choice:
  - ❌ `Momentary Button`, `Toggle Button`, `Timed Move Button`, `Step Angle Button`
  - ✅ `Momentary (Hold to Run)`, `Toggle (Run/Stop)`, `Timed Move`, `Step Angle`

### Rule E: Vertical Compression and Screen Real Estate
- **Top Bar:** Reserve top navigation exclusively for core views and tabs.
- **Footer:** Move passive telemetry (`Battery`, `RTT`, connection status) and emergency brick controls (`STOP ALL`, `Power Off`) to an unobtrusive footer status bar.
- **Above-the-Fold Priority:** Ensure primary action controls (for example, 2D joysticks, sliders) are immediately visible without vertical scrolling on mobile and desktop.

---

## 2. Step-by-Step Audit Procedure

Whenever generating or editing UI templates:

1. **Extract Visible Strings:**
   Inspect all rendered text strings in HTML templates, JavaScript templates, and dynamic renderers.

2. **Frequency Count and Repetition Scan:**
   Run the copy audit helper script:
   ```bash
   python3 .agents/skills/ui-conciseness-audit/scripts/audit_ui_copy.py
   ```
   Or execute a frequency scan across HTML text:
   ```bash
   grep -o -E '\b[A-Za-z]+\b' web_assets/*.html | tr '[:upper:]' '[:lower:]' | sort | uniq -c | sort -nr | head -n 30
   ```

3. **Verify Zero Forbidden Filler Words:**
   Confirm that generic filler words (such as "Mode") appear 0 times in visible text:
   ```bash
   grep -in "mode" web_assets/*.html web_assets/*.js
   ```

4. **Visual Inspection:**
   Render the UI in headless browser simulation, capture desktop and mobile viewports, and visually verify that no two adjacent elements share identical labels.
