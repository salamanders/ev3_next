//! # Sysfs Subsystem Root
//!
//! This module manages Linux sysfs hardware interfaces under `/sys/class` on the EV3 brick.
//! It controls tacho motors, Lego sensors, status LEDs, the framebuffer display console, and Wi-Fi provisioning.
//!
//! In Rust, `mod.rs` serves as the entry point for the `sysfs` module directory.
//! This file declares all hardware child modules and provides shared low-level sysfs helpers.
//! Storing shared file handle caching and port address parsers here prevents code duplication
//! between `motor.rs` and `sensor.rs`, and prevents hardware drivers from importing from the web layer.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::str::FromStr;

pub mod motor;
pub mod sensor;
pub mod mock;
pub mod led;
pub mod display;
pub mod wifi;

pub use motor::{Motor, MotorStatus};
pub use sensor::{Sensor, SensorStatus};
pub use mock::MockController;
pub use led::LedController;
pub use display::DisplayController;
pub use wifi::{WifiManager, WifiProvisionResult};

/// Reads and trims a single-line sysfs attribute file, returning `default` if absent or unreadable.
///
/// Shared by motor and sensor discovery so sysfs attribute reads do not repeat `fs::read_to_string` chains.
pub(crate) fn read_attr_trimmed(dir: &Path, attr: &str, default: &str) -> String {
    fs::read_to_string(dir.join(attr))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| default.into())
}

/// Reads, trims, and parses a typed value from a sysfs attribute file, returning `default` on failure.
///
/// Shared by motor, sensor, and battery sysfs readers for integer and floating-point attributes.
pub(crate) fn read_attr_parsed<T: FromStr>(dir: &Path, attr: &str, default: T) -> T {
    fs::read_to_string(dir.join(attr))
        .ok()
        .and_then(|s| s.trim().parse::<T>().ok())
        .unwrap_or(default)
}

/// Matches Linux kernel sysfs address text against port patterns.
///
/// Cleans the input address string and checks for candidate prefix or suffix tokens.
/// Returns the matching canonical port name (for example, "A" or "1").
///
/// Shared by `parse_motor_address` and `parse_sensor_address` to avoid duplicating string matching logic.
fn parse_channel_address(address: &str, patterns: &[(&str, &str, &'static str)]) -> Option<&'static str> {
    let clean = address.trim().to_ascii_uppercase();
    patterns.iter().find_map(|&(prefix, suffix, port)| {
        if clean.contains(prefix) || clean.ends_with(suffix) {
            Some(port)
        } else {
            None
        }
    })
}

/// Extracts motor port letter ("A", "B", "C", "D") from kernel sysfs address text.
///
/// Parses strings like "ev3-ports:outA" or "outB" from `/sys/class/tacho-motor/motorN/address`.
///
/// Called during motor hardware discovery in `Motor::find_all_in`.
/// Moving this parser from `web/handlers.rs` into `sysfs` keeps hardware driver discovery independent of the web server.
pub fn parse_motor_address(address: &str) -> Option<&'static str> {
    parse_channel_address(address, &[
        ("OUTA", ":A", "A"),
        ("OUTB", ":B", "B"),
        ("OUTC", ":C", "C"),
        ("OUTD", ":D", "D"),
    ])
}

/// Extracts sensor port number ("1", "2", "3", "4") from kernel sysfs address text.
///
/// Parses strings like "ev3-ports:in1" or "in2" from `/sys/class/lego-sensor/sensorN/address`.
///
/// Called during sensor hardware discovery in `Sensor::find_all_in`.
/// Moving this parser from `web/handlers.rs` into `sysfs` keeps hardware driver discovery independent of the web server.
pub fn parse_sensor_address(address: &str) -> Option<&'static str> {
    parse_channel_address(address, &[
        ("IN1", ":1", "1"),
        ("IN2", ":2", "2"),
        ("IN3", ":3", "3"),
        ("IN4", ":4", "4"),
    ])
}

/// Reads a Linux sysfs attribute using a cached file descriptor.
///
/// 1. Opens the target sysfs file on the first call and stores the open `File` handle in `file_opt`.
/// 2. On every call, rewinds the file cursor to byte 0 using `seek(SeekFrom::Start(0))`.
/// 3. Reads updated kernel attribute contents into the provided stack buffer without re-opening the file.
/// 4. If `seek` or `read` fails or returns 0 bytes (for example, when a motor or sensor cable is unplugged),
///    it clears `file_opt` to `None` and returns an error.
///
/// Shared by both `Motor::read_status` (`motor.rs`) and `Sensor::read_status` (`sensor.rs`).
/// Reusing open file handles avoids running `open()` and `close()` kernel syscalls 20 times per second
/// per port on the 300 MHz ARM9 processor, saving substantial CPU time.
pub(crate) fn read_sysfs_cached(
    file_opt: &mut Option<File>,
    path: &Path,
    buf: &mut [u8],
) -> io::Result<usize> {
    if file_opt.is_none() {
        *file_opt = Some(File::open(path)?);
    }
    if let Some(file) = file_opt.as_mut() {
        match file.seek(SeekFrom::Start(0)) {
            Ok(_) => match file.read(buf) {
                Ok(0) => {
                    *file_opt = None;
                    Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty sysfs file"))
                }
                Ok(n) => Ok(n),
                Err(e) => {
                    *file_opt = None;
                    Err(e)
                }
            },
            Err(e) => {
                *file_opt = None;
                Err(e)
            }
        }
    } else {
        Err(io::Error::other("File not available"))
    }
}

/// Writes a Linux sysfs attribute using a cached file descriptor.
///
/// 1. Opens the target sysfs file for writing on the first call and stores the open `File` handle in `file_opt`.
/// 2. On every call, rewinds the file cursor to byte 0 using `seek(SeekFrom::Start(0))`, writes `val`,
///    and truncates to `val.len()` so shorter values cleanly replace longer ones in regular test files.
/// 3. If `seek` or `write_all` fails (for example, when a motor cable is unplugged), it clears `file_opt` to `None`,
///    retries once with a freshly opened handle, and returns any error.
///
/// Reusing open write handles avoids running `open()` and `close()` kernel syscalls on every motor command tick
/// on the 300 MHz ARM9 processor, reducing command latency after idle gaps and during continuous slider drags.
pub(crate) fn write_sysfs_cached(
    file_opt: &mut Option<File>,
    path: &Path,
    val: &[u8],
) -> io::Result<()> {
    if let Some(file) = file_opt.as_mut() {
        if file.seek(SeekFrom::Start(0)).is_ok() && file.write_all(val).is_ok() {
            #[cfg(test)]
            let _ = file.set_len(val.len() as u64);
            return Ok(());
        }
        *file_opt = None;
    }

    let mut file = OpenOptions::new().write(true).open(path)?;
    file.write_all(val)?;
    #[cfg(test)]
    let _ = file.set_len(val.len() as u64);
    *file_opt = Some(file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_motor_address() {
        assert_eq!(parse_motor_address("ev3-ports:outA"), Some("A"));
        assert_eq!(parse_motor_address("outB"), Some("B"));
        assert_eq!(parse_motor_address("OUTC"), Some("C"));
        assert_eq!(parse_motor_address("ev3-ports:outD"), Some("D"));
        assert_eq!(parse_motor_address("outE"), None);
        assert_eq!(parse_motor_address("in1"), None);
    }

    #[test]
    fn test_parse_sensor_address() {
        assert_eq!(parse_sensor_address("ev3-ports:in1"), Some("1"));
        assert_eq!(parse_sensor_address("in2"), Some("2"));
        assert_eq!(parse_sensor_address("IN3"), Some("3"));
        assert_eq!(parse_sensor_address("ev3-ports:in4"), Some("4"));
        assert_eq!(parse_sensor_address("in5"), None);
        assert_eq!(parse_sensor_address("outA"), None);
    }
}
