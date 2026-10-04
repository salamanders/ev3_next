mod config;
mod controller;
mod sysfs;
mod web;

use std::sync::Arc;
use std::thread;
use config::Config;
use controller::MotorController;
use web::Router;

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

    // Check for wifi.txt before asking for Wi-Fi (User Request)
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

    // Periodic LCD display refresh thread (every 5 seconds) (BUG-35)
    let display_clone = sysfs::DisplayController::new(config.mock_mode);
    let controller_disp = controller.clone();
    let port = config.port;
    thread::Builder::new()
        .name("display-refresher".into())
        .spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_secs(5));
                let ip = sysfs::DisplayController::detect_ip();
                let bat = controller_disp.get_battery().voltage_v;
                display_clone.show_ready(&ip, port, bat);
            }
        })
        .expect("Failed to spawn display refresh thread");

    // Worker pool for servicing requests concurrently on ARM/Host (2 workers for single-core CPU)
    let num_workers = 2;
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
                            router_clone.handle_request(request);
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
