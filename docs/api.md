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

## 2. System and Safety Endpoints

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/rescan` | None | Triggers immediate hardware discovery on all 8 ports. |
| `POST` | `/api/estop` | None | **Emergency Stop:** Stops all motors immediately with brake action and sets LEDs to red. |
| `POST` | `/api/emergency-stop` | None | Alias for `/api/estop`. |
| `POST` | `/api/shutdown` | None | Safely powers off the Linux operating system on the EV3 brick. |

---

## 3. Telemetry Endpoints

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

---

## 4. Drive & Legacy Endpoints

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/tank-drive` | `{"left_port":"B", "right_port":"C", "left_speed":500, "right_speed":500}` | Synchronized differential steering. Armed by a 400 ms heartbeat watchdog. |
| `POST` | `/api/motor/{port}/run-forever` | `{"speed": 500}` | Runs motor continuously. |
| `POST` | `/api/motor/{port}/run-timed` | `{"speed": 500, "time_ms": 1000, "stop_action": "brake"}` | Runs motor for duration in ms. |
| `POST` | `/api/motor/{port}/run-to-rel-pos`| `{"speed": 400, "position_sp": 360, "stop_action": "hold"}` | Rotates motor by relative degrees. |
| `POST` | `/api/motor/{port}/run-direct` | `{"duty_cycle": 75}` | Sets direct PWM duty cycle. |
| `POST` | `/api/motor/{port}/stop` | `{"action": "brake"}` | Stops motor (`coast`, `brake`, `hold`). |
| `POST` | `/api/motor/{port}/reset` | None | Resets encoder position count to 0. |
| `POST` | `/api/motor/{port}/polarity` | `{"polarity": "normal" \| "inversed"}` | Inverts motor rotation direction. |
