//! Embeds the icon and version info into the Windows binaries, so Explorer's
//! menu entries, the tray icon and Task Manager show clipsave's icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/clipsave.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    // Resource id 1: `clipsavew.exe,0` in the registry and the tray icon.
    res.set_icon("assets/clipsave.ico");
    res.set("FileDescription", "clipsave");
    res.set("ProductName", "clipsave");
    res.set("LegalCopyright", "MIT License");
    res.compile()
        .expect("cannot embed the Windows icon (is the Windows SDK installed?)");
}
