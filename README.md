# clipsave

**Save the clipboard image as a PNG file, straight from your file manager's right-click menu.**

Copy an image in Figma, a browser, or a screenshot tool, right-click an empty spot in a folder, and pick **Paste image here**. You get `image-20261015-143000.png` in that folder, selected and ready to rename. You don't need to open Paint, use Save As, or browse for the folder.

On Windows you can skip the menu: press **Ctrl+V** in Explorer or on the desktop, the same way you'd paste a file.

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
- **Ctrl+V in Explorer (Windows).** A small background agent makes Ctrl+V save the image into the folder you're looking at. It only acts when Explorer would have nothing to paste, so copying and pasting files works exactly as before.
- **Small and self-contained.** One small binary per OS, no runtime. The Windows exe is about 490 KB.
- **Unicode paths work.** Thai characters and spaces in folder and file names are covered by tests.

## Install

Download the archive for your OS from [Releases](https://github.com/narinthon-so/clipsave/releases), extract it, then:

### Windows 10 / 11

```powershell
.\clipsave.exe install
```

This copies both exes to `%LOCALAPPDATA%\Programs\clipsave`, starts the Ctrl+V agent (and registers it to start at login), and adds two menu entries:

| Right-click on…          | Entry                      | Result                                               |
|--------------------------|----------------------------|------------------------------------------------------|
| empty space in a folder  | **Paste image here**       | saves, selects the file and starts renaming it |
| a folder icon            | **Paste image into folder** | saves into that folder without opening it            |

On Windows 11 these entries are under **Show more options**. **Shift+right-click** opens that menu directly. Windows only lets packaged, signed apps add to the new compact menu, which is why the Ctrl+V agent exists.

#### Ctrl+V in Explorer

With the agent running, **Ctrl+V** in an Explorer window or on the desktop saves the clipboard image there and opens the rename box. It takes over Ctrl+V only when **all** of these are true:

- Explorer or the desktop is the active window, and focus is on the file list. Ctrl+V in the address bar, the search box, the navigation pane or a rename box is left alone.
- The clipboard holds an image and **nothing Explorer can paste itself**: no copied files, no Outlook attachments, no items copied from a zip.
- Only Ctrl is held: Ctrl+Shift+V and the like are left alone.

In every other case the key goes to Explorer untouched. On Windows 11 it uses the tab you're looking at. In folders that aren't on disk (Home, This PC, Gallery) it beeps instead of saving.

**Privacy.** The agent uses a low-level keyboard hook (`WH_KEYBOARD_LL`). It ignores every key except V and never records or sends anything. Read [`src/agent/windows.rs`](src/agent/windows.rs) to check. Antivirus tools sometimes flag unsigned programs that install keyboard hooks. If you'd rather not run it, use `clipsave install --no-agent`, or quit it from its tray icon.

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

> The entry files are checked in CI, but where they show up in each file manager hasn't been tried on a real desktop yet. Bug reports are welcome.

### macOS

```sh
./clipsave install
```

This copies the binary to `~/.local/bin/clipsave` and prints a one-minute Automator recipe that adds a **Quick Action** for folders. Finder doesn't let third-party tools add items to its background menu without a signed Finder extension.

### Uninstall

`clipsave uninstall` removes the menu entries, the Ctrl+V agent and the installed binary.

## CLI

```
clipsave [save] [DIR] [OPTIONS]   Save the clipboard image into DIR (default: current folder)
clipsave install [--no-agent]     Add "Paste image here" to the file manager's right-click menu;
                                  on Windows also start the Ctrl+V agent (and at every login)
clipsave uninstall                Remove both again
clipsave agent [stop]             Run (or stop) the Ctrl+V agent (Windows)

-n, --name <NAME>   File name (".png" is added if missing). Default: image-YYYYMMDD-HHMMSS.png
    --reveal        Select the new file in the open Explorer window and start renaming it (Windows)
    --gui           Report errors in a dialog/notification instead of only on stderr
```

It prints the saved path on success. Exit codes are `0` for saved, `1` if there's no readable image on the clipboard, and `2` for any other error. That makes it easy to script, for example from WSL: `clipsave.exe "$(wslpath -w .)"`.

## How it works

```
right-click ─▶ clipsavew.exe save --gui --reveal "%V"
                 │
                 ├─ resolve_dir   "%V" on a drive root arrives as  D:"  (argv quirk) → D:\
                 ├─ clip          arboard: PNG format ▸ CF_DIBV5   (retries if another app holds the clipboard)
                 ├─ output        RGBA → PNG, create_new image-…png / -1 / -2 …
                 └─ notify        IShellWindows → the open view of that folder → SelectItem(SVSI_EDIT)

Ctrl+V ─▶ clipsavew.exe agent    (keyboard hook, started at login)
            ├─ hook thread    classes + modifiers + clipboard formats only → swallow or pass
            └─ worker thread  window → active tab → folder → same save + select as above
```

| Module | Responsibility |
|---|---|
| [`cli.rs`](src/cli.rs) | argument parsing, no dependencies |
| [`clip.rs`](src/clip.rs) | read the clipboard image, cross-platform via [arboard](https://github.com/1Password/arboard) |
| [`output.rs`](src/output.rs) | folder resolution, file naming, collision-safe writes, PNG encoding |
| [`notify.rs`](src/notify.rs) | dialogs and notifications: `MessageBoxW` / `osascript` / `notify-send`, reveal in Explorer |
| [`integrate/`](src/integrate) | `install`/`uninstall` for Explorer (registry), Nautilus/Nemo/Dolphin, Finder |
| [`agent/rules.rs`](src/agent/rules.rs) | when Ctrl+V is taken over: a pure function, unit-tested on every OS |
| [`agent/windows.rs`](src/agent/windows.rs) | the keyboard hook, worker thread, tray icon, start/stop |
| [`explorer.rs`](src/explorer.rs) | Explorer over COM: folder of a window or tab, select a file in an open view |

Some details that only showed up in testing:

- When you right-click empty space at a drive root, Explorer expands `"%V"` to `"D:\"`. Windows argument parsing reads `\"` as an escaped quote, so the program receives `D:"`. `resolve_dir` repairs this, and a test on a `subst` drive covers it.
- `SHOpenFolderAndSelectItems`, the usual "select this file" call, opens a *second* Explorer window on Windows 11 instead of reusing the one you right-clicked in. clipsave goes through `IShellWindows` instead, finds the view already showing the folder, and selects the file there. Explorer's PIDL for the folder doesn't compare equal to one built from the path, so windows are matched by filesystem path, and the file's item ID is parsed through the view's own `IShellFolder`.
- Every tab of a Windows 11 Explorer window reports the same window handle, and inactive tabs aren't hidden. The active tab is the first `ShellTabWindowClass` child in z-order. When no tab matches, the agent saves nothing rather than guess.
- Starting the agent with `std::process::Command` handed it the installer's stdout pipe. Windows passes *every* inheritable handle to the child, even with null stdio. Any script waiting for `clipsave install`'s output then hung forever. The agent is now started with `CreateProcessW` and handle inheritance off.
- Stripping `.png` from a Thai file name by byte index panics, because the cut lands in the middle of a 3-byte character. The unit tests caught it.

## Tests

```sh
cargo test                                            # unit tests (any OS)
powershell -STA -File tests/e2e-windows.ps1 -Bin target/release   # Windows end-to-end
powershell -STA -File tests/e2e-agent-windows.ps1 -Bin target/release  # Ctrl+V agent (interactive desktop only)
xvfb-run -a bash tests/e2e-unix.sh target/release/clipsave       # Linux end-to-end
bash tests/e2e-unix.sh target/release/clipsave                   # macOS end-to-end
```

The end-to-end scripts put a real image on the system clipboard, run the binary and check the PNG pixel by pixel, including alpha. On Windows the script also installs clipsave and runs the registry command lines exactly as Explorer would, against a Thai-named folder and a drive root. CI runs the same checks on Windows, Ubuntu (X11 under xvfb) and macOS.

The agent test drives a real Explorer window with injected key presses, so it needs an interactive desktop and doesn't run in CI. It covers:
- saving and selecting
- a held Ctrl+V producing exactly one file
- the rename box
- a copied file still pasting normally
- a text clipboard
- a Home tab
- two folder tabs, each saving into its own folder
- stop and restart

## Build

```sh
cargo build --release   # target/release/clipsave(.exe) and clipsavew.exe
```

It builds with stable Rust. Linux needs no system libraries, because the X11 and Wayland clients are pure Rust.

## License

[MIT](LICENSE)
