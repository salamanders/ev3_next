pub struct Config {
    pub host: String,
    pub port: u16,
    pub mock_mode: bool,
    pub poll_interval_ms: u64,
}

impl Config {
    pub fn parse_from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut host = "0.0.0.0".to_string();
        let mut port: u16 = 80;
        let mut port_explicit = false;
        let mut mock_mode = false;
        let mut poll_interval_ms = 50;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--host" => {
                    if i + 1 < args.len() {
                        host = args[i + 1].clone();
                        i += 1;
                    }
                }
                "--port" | "-p" => {
                    if i + 1 < args.len() {
                        if let Ok(p) = args[i + 1].parse::<u16>() {
                            port = p;
                            port_explicit = true;
                        }
                        i += 1;
                    }
                }
                "--mock" | "-m" => {
                    mock_mode = true;
                }
                "--poll-interval" => {
                    if i + 1 < args.len() {
                        if let Ok(interval) = args[i + 1].parse::<u64>() {
                            poll_interval_ms = interval;
                        }
                        i += 1;
                    }
                }
                "--help" | "-h" => {
                    println!("EV3 Web Motor Control Server");
                    println!("Usage: ev3-web-motor [OPTIONS]");
                    println!();
                    println!("Options:");
                    println!("  --host <HOST>           Bind host address (default: 0.0.0.0)");
                    println!("  -p, --port <PORT>       Bind port (default: 8080 in mock mode, 80 on hardware)");
                    println!("  -m, --mock              Force mock hardware simulation mode");
                    println!("  --poll-interval <MS>    Sysfs background polling interval in ms (default: 50)");
                    println!("  -h, --help              Print help information");
                    std::process::exit(0);
                }
                _ => {}
            }
            i += 1;
        }

        // Auto-detect mock mode if on non-Linux or sysfs path does not exist
        if !mock_mode && !std::path::Path::new("/sys/class/tacho-motor").exists() {
            println!("[INFO] Sysfs path /sys/class/tacho-motor not detected. Defaulting to MOCK mode.");
            mock_mode = true;
        }

        // Default to port 8080 for mock development mode, 80 for real hardware on EV3
        if !port_explicit {
            port = if mock_mode { 8080 } else { 80 };
        }

        Config {
            host,
            port,
            mock_mode,
            poll_interval_ms,
        }
    }
}
