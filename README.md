# clipsave

**Save the clipboard image as a PNG file, straight from your file manager's right-click menu.**

Copy an image in Figma, a browser, or a screenshot tool, right-click an empty spot in a folder, and pick **Paste image here**. You get `image-20261015-143000.png` in that folder, selected and ready to rename. You don't need to open Paint, use Save As, or browse for the folder.

Windows Explorer, macOS Finder and most Linux file managers can't paste a clipboard *image* as a file. clipsave adds that, and it also works as a plain CLI:

```console
$ clipsave ~/Desktop --name hero
/home/me/Desktop/hero.png
```

<!-- demo GIF: docs/demo.gif -->

## Features

- **Keeps transparency.** Reads the raw `PNG` clipboard format when the source app publishes it (Figma's *Copy as PNG* does), so alpha is kept exactly. Falls back to `CF_DIBV5` on Windows, `public.png`/TIFF on macOS, and `image/png` on Linux (X11 and Wayland).
- **Never overwrites.** Existing names get a `-1`, `-2`, … suffix. Each attempt uses an atomic `create_new`, so even two saves in the same second can't collide.
- **No admin rights.** Installs per user: `HKCU` registry on Windows, `~/.local` on Linux and macOS.
- **No console flash.** On Windows the menu runs `clipsavew.exe`, a windowless twin of the CLI (like `python`/`pythonw`). Errors appear as a dialog, not a console window that disappears too fast to read.
- **Small and self-contained.** One small binary per OS, no runtime. The Windows exe is about 380 KB.
- **Unicode paths work.** Thai, emoji and spaces in folder and file names are covered by tests.

## Install

Download the archive for your OS from [Releases](https://github.com/narinthon-so/clipsave/releases), extract it, then:

### Windows 10 / 11

```powershell
.\clipsave.exe install
```

This copies both exes to `%LOCALAPPDATA%\Programs\clipsave` and adds two menu entries:

| Right-click on…          | Entry                      | Result                                               |
|--------------------------|----------------------------|------------------------------------------------------|
| empty space in a folder or on the desktop | **Paste image here**       | saves, selects the file and starts renaming it |
| a folder icon            | **Paste image into folder** | saves into that folder without opening it            |

On Windows 11 these entries are under **Show more options**. **Shift+right-click** opens that menu directly. Windows only lets packaged, signed apps add to the new compact menu.

SmartScreen may warn the first time, because the exe isn't code-signed. Choose *More info → Run anyway*, or build it yourself.

### Linux

```sh
./clipsave install
```

This copies the binary to `~/.local/bin/clipsave` and adds a menu entry for each file manager it finds:

| File manager | Where it appears |
|---|---|
| Nautilus (GNOME Files) | right-click → **Scripts → Paste image here** |
| Nemo (Cinnamon) | right-click on folder background |
| Dolphin (KDE) | right-click → **Paste image here** |

Notifications go through `notify-send` when it's available.

### macOS

```sh
./clipsave install
```

This copies the binary to `~/.local/bin/clipsave` and prints a one-minute Automator recipe that adds a **Quick Action** for folders. Finder doesn't let third-party tools add items to its background menu without a signed Finder extension.

### Uninstall

`clipsave uninstall` removes the menu entries and the installed binary.

## CLI

```
clipsave [save] [DIR] [OPTIONS]   Save the clipboard image into DIR (default: current folder)
clipsave install                  Add "Paste image here" to the file manager's right-click menu
clipsave uninstall                Remove it again

-n, --name <NAME>   File name (".png" is added if missing). Default: image-YYYYMMDD-HHMMSS.png
    --reveal        Select the new file in Explorer and start renaming it (Windows)
    --gui           Report errors in a dialog/notification instead of only on stderr
```

It prints the saved path on success. Exit codes are `0` for saved, `1` if there's no image on the clipboard, and `2` for any other error. That makes it easy to script, for example from WSL: `clipsave.exe "$(wslpath -w .)"`.

## How it works

```
right-click ─▶ clipsavew.exe save --gui --reveal "%V"
                 │
                 ├─ resolve_dir   "%V" on a drive root arrives as  D:"  (argv quirk) → D:\
                 ├─ clip          arboard: PNG format ▸ CF_DIBV5   (retries if another app holds the clipboard)
                 ├─ output        RGBA → PNG, create_new image-…png / -1 / -2 …
                 └─ notify        SHOpenFolderAndSelectItems(OFASI_EDIT) → file selected, rename box open
```

| Module | Responsibility |
|---|---|
| [`cli.rs`](src/cli.rs) | argument parsing, no dependencies |
| [`clip.rs`](src/clip.rs) | read the clipboard image, cross-platform via [arboard](https://github.com/1Password/arboard) |
| [`output.rs`](src/output.rs) | folder resolution, file naming, collision-safe writes, PNG encoding |
| [`notify.rs`](src/notify.rs) | dialogs and notifications: `MessageBoxW` / `osascript` / `notify-send`, reveal in Explorer |
| [`integrate/`](src/integrate) | `install`/`uninstall` for Explorer (registry), Nautilus/Nemo/Dolphin, Finder |

Some details that only showed up in testing:

- When you right-click empty space at a drive root, Explorer expands `"%V"` to `"D:\"`. Windows argument parsing reads `\"` as an escaped quote, so the program receives `D:"`. `resolve_dir` repairs this, and a test on a `subst` drive covers it.
- Stripping `.png` from a Thai file name by byte index panics, because the cut lands in the middle of a 3-byte character. The unit tests caught it.

## Tests

```sh
cargo test                                            # unit tests (any OS)
powershell -STA -File tests/e2e-windows.ps1 -Bin target/release   # Windows end-to-end
xvfb-run -a bash tests/e2e-unix.sh target/release/clipsave       # Linux end-to-end
bash tests/e2e-unix.sh target/release/clipsave                   # macOS end-to-end
```

The end-to-end scripts put a real image on the system clipboard, run the binary and check the PNG pixel by pixel, including alpha. On Windows the script also installs clipsave and runs the registry command lines exactly as Explorer would, against a Thai-named folder and a drive root. CI runs all of this on Windows, Ubuntu and macOS for every push.

## Build

```sh
cargo build --release   # target/release/clipsave(.exe) and clipsavew.exe
```

It builds with stable Rust. Linux needs no system libraries, because the X11 and Wayland clients are pure Rust.

## License

[MIT](LICENSE)
