//! Reading the clipboard image.
//!
//! arboard already picks the best source per platform: on Windows the raw
//! registered "PNG" format (what Figma, browsers and Snipping Tool put there,
//! alpha intact) and falls back to `CF_DIBV5`; on macOS NSPasteboard PNG/TIFF;
//! on Linux `image/png` over Wayland data-control or X11.

use std::thread::sleep;
use std::time::Duration;

/// Decoded clipboard image, 8-bit RGBA, row-major.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub enum ReadError {
    NoImage,
    Other(String),
}

pub fn read_image() -> Result<Image, ReadError> {
    // Another app can hold the clipboard open for a moment (Windows allows one
    // opener at a time), so retry briefly before giving up.
    let mut last = String::new();
    for attempt in 0..5 {
        if attempt > 0 {
            sleep(Duration::from_millis(60));
        }
        let result = arboard::Clipboard::new().and_then(|mut cb| cb.get_image());
        match result {
            Ok(img) => {
                return Ok(Image {
                    width: img.width as u32,
                    height: img.height as u32,
                    rgba: img.bytes.into_owned(),
                })
            }
            Err(arboard::Error::ContentNotAvailable) => return Err(ReadError::NoImage),
            Err(arboard::Error::ClipboardOccupied) => last = "the clipboard is busy".into(),
            Err(e) => return Err(ReadError::Other(e.to_string())),
        }
    }
    Err(ReadError::Other(last))
}
