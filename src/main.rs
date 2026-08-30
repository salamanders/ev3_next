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

    let controller = MotorController::new(config.mock_mode, config.poll_interval_ms);
    let router = Arc::new(Router::new(controller));

    let addr = format!("{}:{}", config.host, config.port);
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[FATAL] Failed to bind HTTP server to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    let server = Arc::new(server);
    println!("[INFO] HTTP Server started successfully. Ready for commands.");

    // Worker pool for servicing requests concurrently on ARM/Host
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
        let _ = handle.join();
    }
}
