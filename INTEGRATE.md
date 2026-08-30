Integrate anything in this file into the main PROJECT_PLAN.md
(I will be pasting feedback and discoveries into this file async)

---

Yes, the **`ev3dev-lang-rust`** crate exists and is actively maintained, but **you are not reinventing the wheel for the web server part**.

### What `ev3dev-lang-rust` Handles (Wheel Already Invented)

The crate provides idiomatic Rust wrappers around the Linux kernel sysfs interfaces. You do not need to manually parse file paths like `/sys/class/tacho-motor/motor0/command` or write string buffers.

`ev3dev-lang-rust` provides high-level Rust abstractions for:

* **Tacho Motors:** Direct commands for speed, position, running, and stopping motor outputs across ports A, B, C, and D.


* **Sensors & Peripherals:** Color, gyro, infrared, touch, and ultrasonic sensors.


* **On-Brick Hardware:** EV3 buttons, LEDs, display screen, and speaker audio.


* **Cross-Compilation Setup:** Pre-configured support for the `armv5te-unknown-linux-musleabi` host target.



### What You Still Need to Build

The `ev3dev-lang-rust` crate is strictly a hardware driver abstraction library—it contains **no web server or networking endpoints**.

Your MVP project is to bridge an HTTP interface to those hardware functions. You are simply assembling two existing pieces:

1. **The Web Layer:** A micro HTTP library in Rust (such as `tiny_http` or Rust's built-in `std::net::TcpListener`) to receive HTTP commands from your widget.
2. **The Motor Layer:** Calling `ev3dev-lang-rust` methods inside your HTTP route handlers to control the four motors.



### Why This Makes Your MVP Even Easier

Using `ev3dev-lang-rust` eliminates the need to write custom sysfs file-handling logic.

Instead of opening files and writing string parameters, your server code becomes as simple as this:

```rust
use ev3dev_lang_rust::motors::LargeMotor;

// Initialize motor on Port A
if let Ok(motor) = LargeMotor::get("outA") {
    motor.set_speed_sp(500).ok();
    motor.run_forever().ok();
}

```

By combining `ev3dev-lang-rust` with a tiny HTTP server, you save significant development time while keeping your executable fast, lightweight, and single-file native.