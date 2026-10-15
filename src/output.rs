//! Where the file goes and what it is called. Pure logic, no clipboard access.

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::clip::Image;

/// Turns the DIR argument into an existing folder.
///
/// * none → current directory
/// * a file → its parent folder (a bare `foo.txt` → current directory)
/// * Windows: `D:"` → `D:\` — Explorer expands `"%V"` on a drive root to
///   `"D:\"`, and Windows argv rules read the `\"` as an escaped quote.
///
/// Any other path is used exactly as given: no trimming, no lossy UTF-8.
pub fn resolve_dir(arg: Option<&OsStr>) -> Result<PathBuf, String> {
    let dir = match arg {
        None => current_dir()?,
        Some(raw) if raw.is_empty() => return Err("The folder path is empty.".into()),
        Some(raw) => repair_explorer_arg(raw),
    };

    if dir.is_dir() {
        Ok(dir)
    } else if dir.is_file() {
        match dir.parent() {
            Some(p) if !p.as_os_str().is_empty() => Ok(p.to_path_buf()),
            _ => current_dir(),
        }
    } else {
        Err(format!("Folder not found: {}", dir.display()))
    }
}

fn current_dir() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|e| format!("Cannot read the current folder: {e}"))
}

#[cfg(windows)]
fn repair_explorer_arg(raw: &OsStr) -> PathBuf {
    match raw.to_str() {
        Some(s) if s.ends_with('"') => {
            let mut s = s.trim_end_matches('"').to_owned();
            if s.len() == 2 && s.ends_with(':') {
                s.push('\\');
            }
            PathBuf::from(s)
        }
        _ => PathBuf::from(raw),
    }
}

#[cfg(not(windows))]
fn repair_explorer_arg(raw: &OsStr) -> PathBuf {
    PathBuf::from(raw)
}

/// `image-20261015-143000`, local time.
pub fn default_stem() -> String {
    chrono::Local::now()
        .format("image-%Y%m%d-%H%M%S")
        .to_string()
}

/// Strips a trailing `.png` (any case) and characters that are invalid in
/// Windows file names, so `--name` behaves the same on every OS.
pub fn clean_stem(name: &str) -> Result<String, String> {
    let name = name.trim();
    // `get` (not indexing): the last 4 bytes may split a multi-byte character.
    let name = match name
        .len()
        .checked_sub(4)
        .and_then(|i| Some((i, name.get(i..)?)))
    {
        Some((i, ext)) if ext.eq_ignore_ascii_case(".png") => &name[..i],
        _ => name,
    };
    if name.chars().any(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return Err(format!(
            "Invalid file name: {name:?} (no / \\ : * ? \" < > |)"
        ));
    }
    let name = name.trim_end_matches(['.', ' ']);
    if name.is_empty() {
        return Err("The file name is empty.".into());
    }
    Ok(name.to_owned())
}

/// Candidate paths in order: `stem.png`, `stem-1.png`, `stem-2.png`, ...
pub fn candidate(dir: &Path, stem: &str, n: u32) -> PathBuf {
    if n == 0 {
        dir.join(format!("{stem}.png"))
    } else {
        dir.join(format!("{stem}-{n}.png"))
    }
}

/// Writes `bytes` to the first free candidate path. Never overwrites: each
/// attempt uses `create_new`, so a file that appears between check and write
/// just moves us to the next suffix.
pub fn write_new(dir: &Path, stem: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let stem = clean_stem(stem)?;
    for n in 0..10_000 {
        let path = candidate(dir, &stem, n);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create {}: {e}", path.display())),
        };
        if let Err(e) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(format!("Cannot write {}: {e}", path.display()));
        }
        return Ok(path);
    }
    Err(format!(
        "Too many files named {stem}-N.png in {}",
        dir.display()
    ))
}

pub fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let expected = image.width as usize * image.height as usize * 4;
    if image.width == 0 || image.height == 0 || image.rgba.len() != expected {
        return Err(format!(
            "The clipboard image is malformed ({}x{}, {} bytes).",
            image.width,
            image.height,
            image.rgba.len()
        ));
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, image.width, image.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc
        .write_header()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    writer
        .finish()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clipsave-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_stem_shape() {
        let s = default_stem();
        assert!(s.starts_with("image-"), "{s}");
        assert_eq!(s.len(), "image-20261015-143000".len());
    }

    #[test]
    fn clean_stem_rules() {
        assert_eq!(clean_stem("hero").unwrap(), "hero");
        assert_eq!(clean_stem("hero.PNG").unwrap(), "hero");
        assert_eq!(clean_stem(" logo v2 ").unwrap(), "logo v2");
        assert_eq!(clean_stem("หน้าแรก").unwrap(), "หน้าแรก");
        assert_eq!(clean_stem("trailing...").unwrap(), "trailing");
        assert!(clean_stem("a/b").is_err());
        assert!(clean_stem("a:b").is_err());
        assert!(clean_stem(".png").is_err());
        assert!(clean_stem("   ").is_err());
    }

    #[test]
    fn never_overwrites() {
        let dir = tempdir("collide");
        let a = write_new(&dir, "shot", b"one").unwrap();
        let b = write_new(&dir, "shot", b"two").unwrap();
        let c = write_new(&dir, "shot.png", b"three").unwrap();
        assert_eq!(a.file_name().unwrap(), "shot.png");
        assert_eq!(b.file_name().unwrap(), "shot-1.png");
        assert_eq!(c.file_name().unwrap(), "shot-2.png");
        assert_eq!(fs::read(&a).unwrap(), b"one");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unicode_and_spaces_in_dir() {
        let dir = tempdir("ไทย folder");
        let path = write_new(&dir, "รูป", b"x").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"x");
        assert_eq!(resolve_dir(Some(dir.as_os_str())).unwrap(), dir);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_dir_variants() {
        let dir = tempdir("resolve");
        let file = dir.join("f.txt");
        fs::write(&file, "").unwrap();

        // a file resolves to its folder
        assert_eq!(resolve_dir(Some(file.as_os_str())).unwrap(), dir);
        // a bare file name resolves to the current folder (tests run in the crate root)
        assert_eq!(
            resolve_dir(Some(OsStr::new("Cargo.toml"))).unwrap(),
            std::env::current_dir().unwrap()
        );
        // missing folder
        assert!(resolve_dir(Some(dir.join("nope").as_os_str())).is_err());
        assert!(resolve_dir(Some(OsStr::new(""))).is_err());
        // no arg = cwd
        assert_eq!(resolve_dir(None).unwrap(), std::env::current_dir().unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn explorer_quote_quirks() {
        let dir = tempdir("quote");
        // stray trailing quote from "%V"
        let quoted = format!("{}\"", dir.display());
        assert_eq!(resolve_dir(Some(OsStr::new(&quoted))).unwrap(), dir);
        // Explorer passes `C:"` for "%V" on C:\ — must become C:\
        assert_eq!(
            resolve_dir(Some(OsStr::new("C:\""))).unwrap(),
            PathBuf::from("C:\\")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_paths_are_used_verbatim() {
        use std::os::unix::ffi::OsStrExt;
        let base = tempdir("verbatim");
        // trailing space, a literal quote and non-UTF-8 bytes are all valid names
        for name in [&b"trailing space "[..], b"quote\"", b"latin1-\xe9"] {
            let dir = base.join(OsStr::from_bytes(name));
            fs::create_dir(&dir).unwrap();
            assert_eq!(resolve_dir(Some(dir.as_os_str())).unwrap(), dir);
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn png_roundtrip_keeps_alpha() {
        let image = Image {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0], // opaque red, transparent blue
        };
        let bytes = encode_png(&image).unwrap();
        let decoder = png::Decoder::new(&bytes[..]);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&buf[..info.buffer_size()], &image.rgba[..]);
    }

    #[test]
    fn malformed_image_is_rejected() {
        let bad = Image {
            width: 2,
            height: 2,
            rgba: vec![0; 4],
        };
        assert!(encode_png(&bad).is_err());
    }
}
