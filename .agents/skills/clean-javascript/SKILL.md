---
name: clean-javascript
description: >-
  Enforces zero-duplication, modern ES2025+ syntax, strict single-responsibility boundaries, parameterized consolidation, and concise non-echoing comments for JavaScript code. Use whenever writing, reviewing, or refactoring JavaScript functions, classes, and modules.
---

# Clean JavaScript and Anti-Duplication Craft

This skill guides agents to write concise, modular, and non-redundant JavaScript code using modern standards, strict architectural boundaries, and meaningful comments.

---

## 1. Core Directives

### 1. DRY Principle (Zero Duplication)
- If a logic block, calculation, or UI manipulation appears more than once, extract it into a small, pure helper function or class method.
- Isolate repeated DOM queries, template string interpolations, or data conversions.
- Never duplicate similar branches; combine identical structures.

### 2. Modern Language Features (ES2025+)
- Use modern idioms to compress syntax and improve readability:
  - Higher-order array operations: `map()`, `filter()`, `reduce()`, `some()`, `every()`, `flatMap()`, `findLast()`.
  - Destructuring assignments and rest/spread syntax (`...`).
  - Arrow functions with concise implicit returns.
  - Nullish coalescing (`??`) and optional chaining (`?.`).
  - Logical assignment operators (`??=`, `||=`, `&&=`).
  - Object property shorthand and computed property keys.

### 3. Parameterization Over Duplication
- Do not write separate functions for similar actions.
- Write one unified function that accepts parameters or a configuration object to manage variations.
- Use default parameters to eliminate repetitive conditional checks.

### 4. Direct Response Format (No Boilerplate)
- Skip long conversational explanations and speculative rationale.
- Provide the optimized, refactored code block first.
- Follow immediately with a brief bulleted list that identifies what code was consolidated.

---

## 2. Architectural Boundaries (Single Responsibility)

LLMs frequently lump unrelated concerns into a single class or function. You must define explicit boundaries.

### Boundary Rules: What a Unit Does vs What It Does Not Do

| Responsibility Layer | What It Does | What It Does NOT Do |
| :--- | :--- | :--- |
| **Pure Calculations & Math** | Pure transforms, geometry, unit scaling, clamping. Returns plain values. | Does NOT touch the DOM. Does NOT make network calls. Does NOT access `this` or global state. |
| **Network & Transport** | HTTP/WebSocket transport, payload formatting, error decoding. Returns raw responses or promises. | Does NOT render UI elements. Does NOT handle DOM events. Does NOT maintain UI state. |
| **UI Component Renderers** | Generates HTML or DOM nodes from plain state data. | Does NOT execute network calls. Does NOT process business logic. |
| **Input / Pointer Capture** | Captures raw touch, pointer, or keyboard coordinates and normalizes deltas. | Does NOT calculate motor commands. Does NOT format API payloads. |
| **Application Coordinator** | Connects inputs to state updates and dispatches network commands. | Does NOT contain raw HTML strings. Does NOT compute geometric formulas directly. |

### Where to Draw the Line
1. **Never combine DOM rendering with API requests:** Pass data to renderers; pass action callbacks to controllers.
2. **Never inline math formulas inside event listeners:** Extract calculations to pure helpers that take numbers and return numbers.
3. **Never allow UI cards to manage global sync loops:** Individual components emit events or state changes; the central manager orchestrates timing.

---

## 3. Comment Rules (Non-Echoing & Under 80 Chars)

Comments must explain intent and architectural contrast, never restate identifier names.

### Strict Rules:
1. **Length Limit:** 80 characters or fewer per comment.
2. **Zero Lexical Echo:** Never use any words that appear in the function name, class name, or parameter names.
3. **State Purpose and Contrast:** Explain *why* the unit exists and *what makes it different* from other units.

### Examples:

- ❌ Bad (Lexical Echo):
  ```javascript
  // Updates the slider position from client X and client Y coordinates
  function updateSlider(clientX, clientY) { ... }
  ```
- ✅ Good (Non-Echoing, explains purpose and constraint):
  ```javascript
  // Clamps pointer travel to circular disk to safeguard mechanical limits.
  function updateSlider(clientX, clientY) { ... }
  ```

- ❌ Bad (Lexical Echo):
  ```javascript
  // Joystick class representing the virtual 2D joystick
  class Joystick { ... }
  ```
- ✅ Good (Non-Echoing, explains architectural contrast):
  ```javascript
  // Translates touch drags into throttled differential steering vectors.
  class Joystick { ... }
  ```

- ❌ Bad (Restating code):
  ```javascript
  // Sends an HTTP POST request to the API port endpoint
  async function apiPort(port, payload) { ... }
  ```
- ✅ Good (Explains contrast and safety):
  ```javascript
  // Dispatches hardware commands with automatic watchdog refresh.
  async function apiPort(port, payload) { ... }
  ```

---

## 4. Refactoring Heuristics

When reviewing or generating JavaScript, apply this four-step checklist:

1. **Scan for Structural Twins:**
   - Are multiple event listeners doing 80% identical work? Parameterize into one listener factory.
   - Are multiple functions building HTML cards? Use a single template function driven by widget configuration schemas.

2. **Verify Boundary Isolation:**
   - Does any function both calculate geometry AND update DOM styles? Split the geometry into a pure helper.
   - Does any method read DOM elements AND trigger network fetch calls? Separate data extraction from transport.

3. **Compress with Modern Idioms:**
   - Replace `if (val !== null && val !== undefined)` with `??`.
   - Replace `array.forEach(...)` with `map()`, `filter()`, or `reduce()`.
   - Replace repeated object property lookups with destructuring.

4. **Verify Comment Quality:**
   - Count characters (<= 80).
   - Check against function and variable names for duplicate vocabulary.
   - Ensure the comment answers: "Why does this exist instead of using another component?"
