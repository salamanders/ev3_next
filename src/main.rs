//! # EV3 High-Performance Web Server Runtime Entry Point
//!
//! This module provides the `main()` entry point for the `ev3-web-motor` binary.
//! It boots hardware subsystems, starts the HTTP server, and spawns the multi-threaded runtime.
//!
//! Runtime Thread Model:
//! The system operates using three distinct thread types:
//! 1. Main thread:
//!    Parses command line arguments, checks for `wifi.txt` setup files, binds the `tiny_http` server socket,
//!    renders the initial splash screen on the LCD console (`/dev/tty1`), spawns background threads,
//!    and joins HTTP workers until process shutdown.
//! 2. Background telemetry and watchdog thread (`sysfs-poller`, spawned in `MotorController`):
//!    Polls motor and sensor sysfs attributes every 50 ms into an in-memory `RwLock` cache.
//!    Enforces the 2-second communication safety watchdog, automatically coasting motors if client tabs disconnect.
//! 3. Periodic LCD refresher (`display-refresher`):
//!    Updates the physical LCD screen (`/dev/tty1`) every 5 seconds with current IP address, port, and battery voltage.
//! 4. Background widget updater (`widget-updater`, spawned in `updater`):
//!    Runs once on startup. Waits for an active IP address, checks remote GitHub repository for widget updates,
//!    and writes updated single-file widgets atomically.
//! 5. HTTP worker threads (`http-worker-0`..`http-worker-3`, 4 threads):
//!    Concurrently dequeue incoming HTTP requests from the shared `tiny_http` server queue.
//!    Serve static web assets and status queries directly from RAM without disk or sysfs I/O.
//!
//! In Rust, `main.rs` is the binary root. It initializes components, wires dependencies, and launches threads.

mod config;
mod controller;
mod sysfs;
mod updater;
mod web;

use std::sync::Arc;
use std::thread;
use config::Config;
use controller::MotorController;
use web::Router;

/// Main process entry point for the EV3 web motor server.
///
/// 1. Reads CLI arguments and sets up runtime configuration.
/// 2. Initializes status LEDs to amber and attempts Wi-Fi provisioning if `wifi.txt` exists.
/// 3. Creates `MotorController`, which starts the background 50 ms hardware telemetry poller.
/// 4. Binds the HTTP server socket (port 8080 in mock mode, port 80 on physical hardware).
/// 5. Shows server status and IP address on `/dev/tty1` and turns status LEDs green.
/// 6. Launches background threads for LCD screen refresh and widget self-update checks.
/// 7. Spawns 4 HTTP worker threads to service requests concurrently.
///
/// Binary entry point required by the Rust runtime.
fn main() {
    let config = Config::parse_from_args();

    println!("============================================================");
    println!("  LEGO Mindstorms EV3 - High-Performance Web Motor Control  ");
    println!("============================================================");
    println!("Mode:              {}", if config.mock_mode { "MOCK (Hardware Simulation)" } else { "REAL (Hardware sysfs)" });
    println!("Poll Interval:     {} ms", config.poll_interval_ms);
    println!("Listening Address: http://{}:{}", config.host, config.port);
    println!("Dashboard URL:     http://localhost:{}", config.port);
    println!("============================================================");

    let leds = sysfs::LedController::new(config.mock_mode);
    leds.set_starting();

    // Check for wifi.txt setup files across candidate partitions on boot media
    match sysfs::WifiManager::auto_provision(config.mock_mode) {
        sysfs::WifiProvisionResult::Provisioned { ssid, path } => {
            println!("[WIFI] Auto-provisioned Wi-Fi from {:?} for SSID '{}'", path, ssid);
        }
        sysfs::WifiProvisionResult::NotFound => {
            println!("[WIFI] No wifi.txt detected in candidate root locations.");
        }
        sysfs::WifiProvisionResult::InvalidFormat(err) => {
            eprintln!("[WIFI WARN] Invalid format in wifi.txt: {}", err);
        }
        sysfs::WifiProvisionResult::Error(err) => {
            eprintln!("[WIFI ERROR] Failed provisioning Wi-Fi: {}", err);
        }
    }

    let display = sysfs::DisplayController::new(config.mock_mode);

    let controller = MotorController::new(config.mock_mode, config.poll_interval_ms);
    let router = Arc::new(Router::new(controller.clone()));

    let addr = format!("{}:{}", config.host, config.port);
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => {
            leds.set_ready();
            s
        }
        Err(e) => {
            leds.set_error();
            eprintln!("[FATAL] Failed to bind HTTP server to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    let server = Arc::new(server);
    println!("[INFO] HTTP Server started successfully. Ready for commands.");

    let active_ip = sysfs::DisplayController::detect_ip();
    let battery_v = controller.get_battery().voltage_v;
    display.show_ready(&active_ip, config.port, battery_v);

    // Periodic LCD display refresh thread (every 5 seconds)
    let controller_disp = controller.clone();
    let port = config.port;
    thread::Builder::new()
        .name("display-refresher".into())
        .spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_secs(5));
                let ip = sysfs::DisplayController::detect_ip();
                let bat = controller_disp.get_battery().voltage_v;
                display.show_ready(&ip, port, bat);
            }
        })
        .expect("Failed to spawn display refresh thread");

    println!("[WIDGETS] On-demand updater ready via POST /update.");

    // Worker pool servicing requests concurrently on ARM/Host
    let num_workers = 4;
    let mut handles = Vec::new();

    for worker_id in 0..num_workers {
        let server_clone = server.clone();
        let router_clone = router.clone();

        let handle = thread::Builder::new()
            .name(format!("http-worker-{}", worker_id))
            .spawn(move || {
                loop {
                    match server_clone.recv() {
                        Ok(request) => {
                            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                router_clone.handle_request(request);
                            }));
                            if let Err(panic_err) = res {
                                eprintln!("[ERROR] Worker {} caught panic on request: {:?}", worker_id, panic_err);
                            }
                        }
                        Err(e) => {
                            // Server might be shutting down
                            eprintln!("[WARN] Worker {} error receiving request: {}", worker_id, e);
                            break;
                        }
                    }
                }
            })
            .expect("Failed to spawn HTTP worker thread");

        handles.push(handle);
    }

    for handle in handles {
        if let Err(e) = handle.join() {
            eprintln!("[ERROR] HTTP worker thread panicked: {:?}", e);
        }
    }
}
