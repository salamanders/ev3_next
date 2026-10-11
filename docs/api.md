# EV3 REST API Reference

The EV3 Web Motor Control server exposes a JSON REST API for controlling motors, querying sensor data, and monitoring hardware health.

---

## 1. Unified Port API (`/api/port`)

The unified port endpoint handles all motors (ports `A`–`D`) and sensors (ports `1`–`4`) using a single interface. Port names are case-insensitive and accept short (`A`, `1`) or long (`outA`, `in1`) identifiers.

### Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/ports` | Returns status and telemetry for all 8 ports from in-memory cache. |
| `GET` | `/api/port/{port}` | Returns telemetry for a single port from in-memory cache. |
| `POST` | `/api/port/{port}` | Executes motor movement or sets sensor mode. |
| `POST` | `/api/batch` | Dispatches multi-port commands in a single atomic request with optional heartbeat watchdog. |
| `POST` | `/api/ports` | Alias for `POST /api/batch`. |

### Payload Attributes (`POST /api/port/{port}`)

The endpoint accepts raw sysfs keys and convenience aliases:

| Key | Aliases | Type | Description |
| :--- | :--- | :--- | :--- |
| `command` | — | String | Motor command (`run-forever`, `run-timed`, `run-to-rel-pos`, `run-direct`, `stop`, `reset`). |
| `speed_sp` | `speed` | Integer | Target velocity in encoder ticks/second (-1560 to +1560). |
| `time_sp` | `duration_s`, `time_ms` | Float / Int | Duration for timed runs (seconds or milliseconds). |
| `position_sp` | `degrees`, `angle` | Integer | Relative rotation angle in degrees. |
| `duty_cycle_sp`| `duty` | Integer | Direct PWM duty cycle (-100 to +100). |
| `stop_action` | `stop_mode` | String | Stop behavior (`coast`, `brake`, `hold`). |
| `polarity` | — | String | Motor polarity (`normal`, `inversed`). |
| `mode` | — | String | Sensor operating mode (e.g., `TOUCH`, `COL-COLOR`, `US-DIST-CM`, `GYRO-ANG`). |

### Examples

**Continuous Run:**
```bash
curl -X POST http://<ip>/api/port/A \
  -H "Content-Type: application/json" \
  -d '{"speed": 700}'
```

**Timed Move (2.5 seconds):**
```bash
curl -X POST http://<ip>/api/port/B \
  -H "Content-Type: application/json" \
  -d '{"speed": 500, "duration_s": 2.5, "stop_action": "brake"}'
```

**Relative Angle Turn (90 degrees):**
```bash
curl -X POST http://<ip>/api/port/D \
  -H "Content-Type: application/json" \
  -d '{"speed": 400, "degrees": 90, "stop_action": "hold"}'
```

**Set Sensor Mode:**
```bash
curl -X POST http://<ip>/api/port/3 \
  -H "Content-Type: application/json" \
  -d '{"mode": "US-DIST-CM"}'
```

---

## 2. Batch Port Control (`POST /api/batch`, `POST /api/ports`)

The batch endpoint executes commands across multiple ports in a single atomic HTTP request. It removes network round-trip skew between coordinated motors and allows client widgets to control arbitrary combinations of motors without server-side Rust changes.

### Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `POST` | `/api/batch` | Dispatches multi-port commands with an optional heartbeat safety watchdog. |
| `POST` | `/api/ports` | Alias for `POST /api/batch`. |

### Payload Structure

| Key | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `heartbeat_ms` | Integer | No | Watchdog timeout in milliseconds (clamped between `100` and `5000`). If omitted, no heartbeat timeout is set. |
| `commands` | Array of Objects | Yes | List of port commands to execute sequentially in the batch. |

#### Command Object Attributes (`commands[]`)

Each object in the `commands` array specifies a target port and the motor or sensor action:

| Key | Aliases | Type | Description |
| :--- | :--- | :--- | :--- |
| `port` | — | String | **Required.** Port identifier (case-insensitive: `A`–`D`, `1`–`4`, `outA`–`outD`, `in1`–`in4`). |
| `speed_sp` | `speed` | Integer | Target velocity in encoder ticks/second (-1560 to +1560). |
| `time_sp` | `duration_s`, `time_ms` | Float / Int | Duration for timed run (seconds or milliseconds). |
| `position_sp` | `degrees`, `angle` | Integer | Relative rotation angle in degrees. |
| `duty_cycle_sp`| `duty` | Integer | Direct PWM duty cycle (-100 to +100). |
| `stop_action` | `stop_mode` | String | Stop behavior (`coast`, `brake`, `hold`). |
| `polarity` | — | String | Motor polarity (`normal`, `inversed`). |
| `command` | — | String | Explicit sysfs command (`run-forever`, `run-timed`, `run-to-rel-pos`, `run-direct`, `stop`, `reset`). |
| `mode` | — | String | Sensor mode (for input ports `1`–`4`). |

### Safety Watchdog Behavior

When `heartbeat_ms` is set:
1. The server arms an in-memory safety timer for all moving motor ports in the batch.
2. If the client stops sending requests (for example, network drop, closed tab, or client freeze) and the timer expires, the background supervisor thread automatically halts all motors across all ports using `coast` action to reduce mechanical stress.
3. Sending a new batch request with `heartbeat_ms` resets the watchdog timer.
4. Sending zero speed stops the motors and disarms the heartbeat watchdog.

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

**Timed Coordinated Movement:**
```bash
curl -X POST http://<ip>/api/batch \
  -H "Content-Type: application/json" \
  -d '{
    "commands": [
      { "port": "A", "speed": 500, "duration_s": 1.5, "stop_action": "brake" },
      { "port": "B", "speed": -500, "duration_s": 1.5, "stop_action": "brake" }
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
| `POST` | `/api/joystick` | `{"x_port":"B","y_port":"C","x_speed":400,"y_speed":-200,"stop_action":"brake"}` | **2D Joystick Control:** Dispatches independent X and Y motor velocities with watchdog protection. |
| `POST` | `/api/rescan` | None | Triggers immediate hardware discovery on all 8 ports. |
| `POST` | `/api/estop` | None | **Emergency Stop:** Stops all motors immediately with brake action and sets LEDs to red. |
| `POST` | `/api/emergency-stop` | None | Alias for `/api/estop`. |
| `POST` | `/api/shutdown` | None | Safely powers off the Linux operating system on the EV3 brick. |

---

## 4. Telemetry Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/status` | Returns system telemetry (motors, sensors, battery) from in-memory cache. |
| `GET` | `/api/telemetry` | Alias for `/api/status`. |
| `GET` | `/api/battery` | Returns battery voltage (`V`) and current (`A`). |

### Envelope Format

All responses use this standard JSON structure:
```json
{
  "success": true,
  "data": { ... }
}
```

### Sample Response (`GET /api/battery`):
```json
{
  "success": true,
  "data": {
    "voltage_v": 7.8,
    "current_a": 0.12
  }
}
```
