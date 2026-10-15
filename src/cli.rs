use std::ffi::OsString;

pub const HELP: &str = "\
clipsave — save the clipboard image as a PNG file

USAGE:
    clipsave [save] [DIR] [OPTIONS]   Save the clipboard image into DIR (default: current folder)
    clipsave install                  Add \"Paste image here\" to the file manager's right-click menu
    clipsave uninstall                Remove it again

OPTIONS:
    -n, --name <NAME>   File name (\".png\" is added if missing). Default: image-YYYYMMDD-HHMMSS.png
        --reveal        Select the new file in Explorer and start renaming it (Windows)
        --gui           Report errors in a dialog/notification instead of only on stderr
    -h, --help          Show this help
    -V, --version       Show the version

Existing files are never overwritten: a -1, -2, ... suffix is added instead.
Prints the saved path on success. Exit code 1 = no image on the clipboard, 2 = other error.";

#[derive(Debug, PartialEq)]
pub enum Command {
    Save(SaveOpts),
    Install,
    Uninstall,
    Help,
    Version,
}

#[derive(Debug, Default, PartialEq)]
pub struct SaveOpts {
    pub dir: Option<OsString>,
    pub name: Option<String>,
    pub reveal: bool,
    pub gui: bool,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args: Vec<OsString> = args.into_iter().collect();

    match args.first().and_then(|a| a.to_str()) {
        Some("install") => return only(args, Command::Install),
        Some("uninstall") => return only(args, Command::Uninstall),
        Some("help") => return Ok(Command::Help),
        Some("save") => {
            args.remove(0);
        }
        _ => {}
    }

    let mut opts = SaveOpts::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
            Some("--reveal") => opts.reveal = true,
            Some("--gui") => opts.gui = true,
            Some("-n" | "--name") => {
                let value = iter.next().ok_or("--name needs a value")?;
                opts.name = Some(
                    value
                        .into_string()
                        .map_err(|_| "--name is not valid Unicode")?,
                );
            }
            Some(s) if s.starts_with("--name=") => {
                opts.name = Some(s["--name=".len()..].to_owned())
            }
            Some(s) if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option: {s}"))
            }
            _ if opts.dir.is_none() => opts.dir = Some(arg),
            _ => return Err(format!("unexpected argument: {}", arg.to_string_lossy())),
        }
    }
    Ok(Command::Save(opts))
}

fn only(args: Vec<OsString>, cmd: Command) -> Result<Command, String> {
    match args.get(1) {
        None => Ok(cmd),
        Some(extra) => Err(format!("unexpected argument: {}", extra.to_string_lossy())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_args_saves_into_current_dir() {
        assert_eq!(p(&[]), Ok(Command::Save(SaveOpts::default())));
    }

    #[test]
    fn save_with_dir_and_flags() {
        let cmd = p(&[
            "save",
            r"C:\Users\me\Desktop",
            "--reveal",
            "--gui",
            "-n",
            "hero",
        ])
        .unwrap();
        assert_eq!(
            cmd,
            Command::Save(SaveOpts {
                dir: Some(r"C:\Users\me\Desktop".into()),
                name: Some("hero".into()),
                reveal: true,
                gui: true,
            })
        );
    }

    #[test]
    fn save_keyword_is_optional() {
        assert_eq!(p(&["save", "out"]), p(&["out"]));
    }

    #[test]
    fn name_equals_form() {
        let Command::Save(opts) = p(&["--name=logo"]).unwrap() else {
            panic!()
        };
        assert_eq!(opts.name.as_deref(), Some("logo"));
    }

    #[test]
    fn folder_named_like_a_command_needs_save() {
        let Command::Save(opts) = p(&["save", "install"]).unwrap() else {
            panic!()
        };
        assert_eq!(opts.dir, Some("install".into()));
    }

    #[test]
    fn errors() {
        assert!(p(&["--bogus"]).is_err());
        assert!(p(&["a", "b"]).is_err());
        assert!(p(&["--name"]).is_err());
        assert!(p(&["install", "x"]).is_err());
    }

    #[test]
    fn help_and_version() {
        assert_eq!(p(&["-h"]), Ok(Command::Help));
        assert_eq!(p(&["help"]), Ok(Command::Help));
        assert_eq!(p(&["save", "--version"]), Ok(Command::Version));
    }
}
