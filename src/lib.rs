//! clipsave — save the clipboard image as a PNG file.
//!
//! The binaries are thin wrappers around [`run`]; everything platform-specific
//! lives in `clip` (reading the clipboard), `notify` (dialogs/toasts) and
//! `integrate` (file-manager context menus).

mod agent;
mod cli;
mod clip;
#[cfg(windows)]
mod explorer;
mod integrate;
mod notify;
mod output;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use cli::{AgentAction, Command};

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
        Command::Install { agent } => finish(integrate::install(agent), windowless),
        Command::Uninstall => finish(integrate::uninstall(), windowless),
        Command::Agent(AgentAction::Run) => finish(agent::run(), windowless),
        Command::Agent(AgentAction::Stop) => {
            let msg = if agent::stop() {
                "Stopped the agent."
            } else {
                "The agent is not running."
            };
            finish(Ok(msg.into()), windowless)
        }
    }
}

pub(crate) struct SaveError {
    pub code: i32,
    pub message: String,
}

impl SaveError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        SaveError {
            code,
            message: message.into(),
        }
    }
}

/// Reads the clipboard image and writes it into `dir` as a new PNG.
pub(crate) fn save_clipboard(dir: &Path, name: Option<String>) -> Result<PathBuf, SaveError> {
    let image =
        match clip::read_image() {
            Ok(image) => image,
            Err(clip::ReadError::NoImage) => return Err(SaveError::new(
                EXIT_NO_IMAGE,
                "There is no image on the clipboard (or not in a format clipsave can read).\n\n\
                 Copy an image first (in Figma: Copy as PNG, Ctrl+Shift+C).",
            )),
            Err(clip::ReadError::Other(msg)) => {
                return Err(SaveError::new(
                    EXIT_ERROR,
                    format!("Could not read the clipboard: {msg}"),
                ))
            }
        };
    let png = output::encode_png(&image).map_err(|m| SaveError::new(EXIT_ERROR, m))?;
    let stem = name.unwrap_or_else(output::default_stem);
    output::write_new(dir, &stem, &png).map_err(|m| SaveError::new(EXIT_ERROR, m))
}

fn save(opts: cli::SaveOpts, windowless: bool) -> i32 {
    let gui = opts.gui || windowless;

    let result = output::resolve_dir(opts.dir.as_deref())
        .map_err(|m| SaveError::new(EXIT_ERROR, m))
        .and_then(|dir| save_clipboard(&dir, opts.name));
    let path = match result {
        Ok(path) => path,
        Err(e) => {
            report_error(&e.message, gui);
            return e.code;
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
            if !msg.is_empty() {
                say(&msg, windowless);
            }
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
