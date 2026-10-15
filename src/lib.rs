//! clipsave — save the clipboard image as a PNG file.
//!
//! The binaries are thin wrappers around [`run`]; everything platform-specific
//! lives in `clip` (reading the clipboard), `notify` (dialogs/toasts) and
//! `integrate` (file-manager context menus).

mod cli;
mod clip;
mod integrate;
mod notify;
mod output;

use std::ffi::OsString;
use std::path::PathBuf;

use cli::Command;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Exit codes.
pub const EXIT_OK: i32 = 0;
pub const EXIT_NO_IMAGE: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

/// Entry point shared by `clipsave` and `clipsavew`.
///
/// `windowless` is true for `clipsavew`, which has no console: every message
/// then goes through a dialog instead of stdout/stderr.
pub fn run(args: impl IntoIterator<Item = OsString>, windowless: bool) -> i32 {
    let cmd = match cli::parse(args) {
        Ok(cmd) => cmd,
        Err(msg) => {
            report_error(
                &format!("{msg}\n\nRun `clipsave --help` for usage."),
                windowless,
            );
            return EXIT_ERROR;
        }
    };

    match cmd {
        Command::Help => {
            say(cli::HELP, windowless);
            EXIT_OK
        }
        Command::Version => {
            say(&format!("clipsave {VERSION}"), windowless);
            EXIT_OK
        }
        Command::Save(opts) => save(opts, windowless),
        Command::Install => finish(integrate::install(), windowless),
        Command::Uninstall => finish(integrate::uninstall(), windowless),
    }
}

fn save(opts: cli::SaveOpts, windowless: bool) -> i32 {
    let gui = opts.gui || windowless;

    let dir = match output::resolve_dir(opts.dir.as_deref()) {
        Ok(dir) => dir,
        Err(msg) => {
            report_error(&msg, gui);
            return EXIT_ERROR;
        }
    };

    let image = match clip::read_image() {
        Ok(image) => image,
        Err(clip::ReadError::NoImage) => {
            report_error(
                "There is no image on the clipboard (or not in a format clipsave can read).\n\n\
                 Copy an image first (in Figma: Copy as PNG, Ctrl+Shift+C).",
                gui,
            );
            return EXIT_NO_IMAGE;
        }
        Err(clip::ReadError::Other(msg)) => {
            report_error(&format!("Could not read the clipboard: {msg}"), gui);
            return EXIT_ERROR;
        }
    };

    let png = match output::encode_png(&image) {
        Ok(png) => png,
        Err(msg) => {
            report_error(&msg, gui);
            return EXIT_ERROR;
        }
    };

    let stem = opts.name.unwrap_or_else(output::default_stem);
    let path: PathBuf = match output::write_new(&dir, &stem, &png) {
        Ok(path) => path,
        Err(msg) => {
            report_error(&msg, gui);
            return EXIT_ERROR;
        }
    };

    if !windowless {
        println!("{}", path.display());
    }
    if opts.reveal {
        notify::reveal(&path);
    } else if gui {
        notify::saved(&path);
    }
    EXIT_OK
}

fn finish(result: Result<String, String>, windowless: bool) -> i32 {
    match result {
        Ok(msg) => {
            say(&msg, windowless);
            EXIT_OK
        }
        Err(msg) => {
            report_error(&msg, windowless);
            EXIT_ERROR
        }
    }
}

fn say(msg: &str, windowless: bool) {
    if windowless {
        notify::info(msg);
    } else {
        println!("{msg}");
    }
}

fn report_error(msg: &str, gui: bool) {
    eprintln!("clipsave: {msg}");
    if gui {
        notify::error(msg);
    }
}
