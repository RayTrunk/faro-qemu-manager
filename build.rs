#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("FileDescription", "FARO QEMU MANAGER");
    res.set("ProductName", "FARO QEMU MANAGER");
    res.set("ProductVersion", "0.9.0.0");
    res.set("FileVersion", "0.9.0.0");
    res.set("LegalCopyright", "(C) R.Trunk 2026");
    res.set("CompanyName", "R.Trunk");
    res.set("OriginalFilename", "faro-qem.exe");
    res.set("InternalName", "faro-qem");
    res.set(
        "Comments",
        "FARO QEMU MANAGER - Modern QEMU Virtual Machine Manager",
    );

    if let Err(e) = res.compile() {
        eprintln!("Warning: Failed to compile Windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
