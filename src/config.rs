//! # Zero-Dependency Command Line Configuration
//!
//! This module parses command line options for the `ev3-web-motor` executable.
//! It configures the bind host address, port, mock simulation mode, and background polling interval.
//!
//! Using a zero-dependency CLI parser saves approximately 400 KB of binary size
//! compared to heavyweight libraries such as `clap`, preserving memory and flash space on the 64 MB EV3.
//! Isolating configuration parsing in `config.rs` allows easy unit testing and clean decoupling from server startup.

/// Runtime configuration parsed from command line flags.
pub struct Config {
    pub host: String,
    pub port: u16,
    pub mock_mode: bool,
    pub poll_interval_ms: u64,
}

impl Config {
    /// Gathers process command line arguments and delegates to parser.
    ///
    /// Collects `std::env::args()` into a string vector and passes them to `parse_from`.
    ///
    /// Production entry point called at the start of `main()`.
    pub fn parse_from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        Self::parse_from(&args)
    }

    /// Parses command line tokens into structured configuration settings.
    ///
    /// 1. Iterates over argument strings, matching `--host`, `-p`/`--port`, `-m`/`--mock`, and `--poll-interval`.
    /// 2. Automatically enables `mock_mode` if `/sys/class/tacho-motor` does not exist on disk.
    /// 3. Defaults port to 8080 when running in mock simulation mode, or port 80 on physical hardware (Rule 13).
    /// 4. Clamps the background telemetry polling interval between 10 ms and 1000 ms.
    ///
    /// Allows unit tests to verify flag combinations with arbitrary argument slices
    /// without mutating global process environment state.
    pub fn parse_from(args: &[String]) -> Self {
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
                            poll_interval_ms = interval.clamp(10, 1000);
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

        // Auto-detect mock mode if on non-Linux system or sysfs path does not exist
        if !mock_mode && !std::path::Path::new("/sys/class/tacho-motor").exists() {
            println!("[INFO] Sysfs path /sys/class/tacho-motor not detected. Defaulting to MOCK mode.");
            mock_mode = true;
        }

        // Fixed Port Rule (Rule 13): Defaults to 8080 for mock development, 80 for hardware.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_poll_interval_clamping() {
        let args = vec![
            "ev3-web-motor".into(),
            "--poll-interval".into(),
            "0".into(),
        ];
        let cfg = Config::parse_from(&args);
        assert_eq!(cfg.poll_interval_ms, 10, "Poll interval 0 must be clamped to 10ms");

        let args_high = vec![
            "ev3-web-motor".into(),
            "--poll-interval".into(),
            "5000".into(),
        ];
        let cfg_high = Config::parse_from(&args_high);
        assert_eq!(cfg_high.poll_interval_ms, 1000, "Poll interval 5000 must be clamped to 1000ms");
    }

    #[test]
    fn test_config_mock_mode_and_port() {
        let args = vec![
            "ev3-web-motor".into(),
            "--mock".into(),
            "--port".into(),
            "9090".into(),
        ];
        let cfg = Config::parse_from(&args);
        assert!(cfg.mock_mode);
        assert_eq!(cfg.port, 9090);
    }
}
