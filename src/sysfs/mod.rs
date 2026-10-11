use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub mod motor;
pub mod sensor;
pub mod mock;
pub mod led;
pub mod display;
pub mod wifi;
pub use motor::{Motor, MotorStatus};
pub use sensor::{Sensor, SensorStatus};
pub use mock::MockController;
#[allow(unused_imports)]
pub use led::{LedColor, LedController};
pub use display::DisplayController;
pub use wifi::{WifiManager, WifiProvisionResult};

// Reuses open descriptors to sample sysfs nodes without file re-opening.
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

