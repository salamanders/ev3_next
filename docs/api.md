# EV3 REST API Reference

The EV3 Web Motor Control server exposes a JSON REST API for controlling motors, querying sensor data, and monitoring hardware health.

---

## 1. Unified Port API (`POST /api/port/{port}`)

The unified port endpoint controls motors (ports `A`–`D`) and sensors (ports `1`–`4`) using a single interface. Port names are case-insensitive and accept short (`A`, `1`) or long (`outA`, `in1`) identifiers.

### Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `POST` | `/api/port/{port}` | Executes motor movement or sets sensor mode. |

### Payload Attributes (`POST /api/port/{port}`)

| Key | Aliases | Type | Description |
| :--- | :--- | :--- | :--- |
| `command` | — | String | Motor command (`run-forever`, `run-to-rel-pos`, `step`, `stop`, `reset`, `mode`). |
| `speed` | `speed_sp` | Integer | Target velocity in encoder ticks/second (-1560 to +1560). |
| `degrees` | `position_sp`, `angle` | Integer | Relative rotation angle in degrees or counts. |
| `stop_action` | `stop_mode` | String | Stop behavior (`coast`, `brake`, `hold`). Default is `brake`. |
| `mode` | — | String | Sensor operating mode (e.g., `TOUCH`, `COL-COLOR`, `US-DIST-CM`, `GYRO-ANG`). |

### Examples

**Continuous Run:**
```bash
curl -X POST http://<ip>/api/port/A \
  -H "Content-Type: application/json" \
  -d '{"speed": 700}'
```

**Relative Angle Turn (90 degrees):**
```bash
curl -X POST http://<ip>/api/port/D \
  -H "Content-Type: application/json" \
  -d '{"speed": 400, "degrees": 90, "stop_action": "hold"}'
```

**Halt Motor:**
```bash
curl -X POST http://<ip>/api/port/A \
  -H "Content-Type: application/json" \
  -d '{"command": "stop", "stop_action": "coast"}'
```

**Set Sensor Mode:**
```bash
curl -X POST http://<ip>/api/port/3 \
  -H "Content-Type: application/json" \
  -d '{"mode": "US-DIST-CM"}'
```

---

## 2. Batch Port Control (`POST /api/batch`)

The batch endpoint executes commands across multiple ports in a single atomic HTTP request. It removes network round-trip skew between coordinated motors and allows client widgets to control arbitrary combinations of motors without server-side Rust changes.

### Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `POST` | `/api/batch` | Dispatches multi-port commands with an optional heartbeat safety watchdog. |

### Payload Structure

| Key | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `heartbeat_ms` | Integer | No | Watchdog timeout in milliseconds (clamped between `100` and `5000`). If omitted, no custom heartbeat timeout is set. |
| `commands` | Array of Objects | Yes | List of port commands to execute in the batch. |

#### Command Object Attributes (`commands[]`)

Each object in the `commands` array specifies a target port and the motor or sensor action:

| Key | Aliases | Type | Description |
| :--- | :--- | :--- | :--- |
| `port` | — | String | **Required.** Port identifier (case-insensitive: `A`–`D`, `1`–`4`, `outA`–`outD`, `in1`–`in4`). |
| `speed` | `speed_sp` | Integer | Target velocity in encoder ticks/second (-1560 to +1560). |
| `degrees` | `position_sp`, `angle` | Integer | Relative rotation angle in degrees or counts. |
| `stop_action` | `stop_mode` | String | Stop behavior (`coast`, `brake`, `hold`). Default is `brake`. |
| `command` | — | String | Motor command (`run-forever`, `run-to-rel-pos`, `step`, `stop`, `reset`, `mode`). |
| `mode` | — | String | Sensor mode (for input ports `1`–`4`). |

### Safety Watchdog Behavior

All driving operations are protected by the unified safety watchdog (2000 ms default cutoff):
1. The server arms the unified safety timer when any motor starts moving.
2. If client communication ceases (for example, network drop, closed tab, or client freeze) and the 2.0-second timer expires, the background supervisor thread automatically halts all motors across all ports using `coast` action to reduce mechanical stress.
3. Any client communication (such as telemetry polling or command dispatches) resets the watchdog timer.
4. Sending zero speed stops the motors and disarms the safety watchdog.

### Examples

**Dual-Motor Drive (2D Joystick / Independent Axes):**
```bash
curl -X POST http://<ip>/api/batch \
  -H "Content-Type: application/json" \
  -d '{
    "heartbeat_ms": 2000,
    "commands": [
      { "port": "B", "speed": 600, "stop_action": "coast" },
      { "port": "C", "speed": 400, "stop_action": "brake" }
    ]
  }'
```

**Multi-Axis Robot Arm (3 Motors):**
```bash
curl -X POST http://<ip>/api/batch \
  -H "Content-Type: application/json" \
  -d '{
    "heartbeat_ms": 500,
    "commands": [
      { "port": "A", "speed": 200, "stop_action": "hold" },
      { "port": "B", "speed": -150, "stop_action": "hold" },
      { "port": "D", "speed": 100, "stop_action": "hold" }
    ]
  }'
```

### Response Format

**Success Response (`200 OK`):**
```json
{
  "success": true,
  "data": [
    "Motor on Port B running forever at speed 600",
    "Motor on Port C running forever at speed 400"
  ]
}
```

**Error Response (`400 Bad Request` or `500 Internal Server Error`):**
```json
{
  "success": false,
  "error": "Invalid port 'Z'"
}
```

---

## 3. System and Safety Endpoints

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/rescan` | None | Triggers immediate hardware discovery on all 8 ports and returns updated system status. |
| `POST` | `/api/estop` | None | **Emergency Stop:** Stops all motors immediately with brake action. |
| `POST` | `/api/shutdown` | None | Safely powers off the Linux operating system on the EV3 brick. |

---

## 4. Telemetry Endpoint

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/status` | Returns system telemetry (motors, sensors, battery) from in-memory cache. |

### Envelope Format

All responses use this standard JSON structure:
```json
{
  "success": true,
  "data": { ... }
}
```

### Sample Response (`GET /api/status`):
```json
{
  "success": true,
  "data": {
    "motors": [
      {
        "port": "A",
        "name": "motor0",
        "driver_name": "lego-ev3-l-motor",
        "address": "outA",
        "speed": 0,
        "position": 120,
        "max_speed": 1050,
        "duty_cycle": 0,
        "state": [],
        "polarity": "normal",
        "stop_action": "brake",
        "connected": true
      }
    ],
    "sensors": [
      {
        "port": "1",
        "name": "sensor0",
        "driver_name": "lego-ev3-touch",
        "address": "in1",
        "mode": "TOUCH",
        "modes": ["TOUCH"],
        "units": "",
        "decimals": 0,
        "value0": 0.0,
        "connected": true
      }
    ],
    "battery": {
      "voltage_v": 7.8,
      "current_a": 0.12
    }
  }
}
```

---

## 5. Modular Widget Endpoints (`/widgets/*`)

The HTTP server delivers dynamic widget components directly from local storage or embedded compile-time RAM:

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` / `HEAD` | `/widgets/manifest.json` | Catalog of available widgets, version strings, and timestamps. |
| `GET` / `HEAD` | `/widgets/{widget}/widget.js` | Modular native ES component implementation. |

- **Plain Delivery:** Serves uncompressed files directly from disk if present; otherwise serves embedded baseline widgets from RAM.
- **Single-File Architecture:** Each modular widget consists of a single `widget.js` file.
- **Cache Policy:** Emits `Cache-Control: no-cache` headers on all `/widgets/*` responses.
