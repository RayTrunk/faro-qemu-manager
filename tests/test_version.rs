// FARO QEMU MANAGER (C) R.Trunk 2026
// The version number lives in two files that no compiler compares, and
// the licence has to be reachable from inside the running program.

mod common;

use egui::Context;
use tempfile::tempdir;

/// Windows reads the version from the resource `build.rs` compiles into
/// the executable; cargo reads it from `Cargo.toml`. Nothing connects the
/// two. Raise one and forget the other, and the file properties in the
/// Explorer name a build that does not exist -- the first thing anyone
/// looks at when they report a fault.
#[test]
fn test_the_windows_resource_carries_the_same_version_as_the_package() {
    let build_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"))
        .expect("build.rs sits next to Cargo.toml");
    let package = env!("CARGO_PKG_VERSION");
    // Windows version resources are four-part; ours is the package version
    // with a build number of zero.
    let expected = format!("{package}.0");

    for field in ["ProductVersion", "FileVersion"] {
        let needle = format!("res.set(\"{field}\", \"{expected}\")");
        assert!(
            build_rs.contains(&needle),
            "build.rs does not set {field} to {expected} (the package says {package})"
        );
    }
}

/// The GPL asks that a program under it tell the person running it so,
/// and the About dialog is where anyone looks for that. The same window
/// answers "which build is this?", so both belong in it.
#[test]
fn test_the_about_dialog_names_the_version_and_the_licence() {
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("cfg"));
    app.about_open = true;

    let ctx = Context::default();
    // Two frames: the first lays the window out, the second draws into a
    // settled layout.
    let _ = ctx.run(Default::default(), |ctx| app.render_ui(ctx));
    let output = ctx.run(Default::default(), |ctx| app.render_ui(ctx));
    let drawn = common::rendered_text(&output);

    assert!(
        drawn.contains(env!("CARGO_PKG_VERSION")),
        "the About dialog never names version {}: {}",
        env!("CARGO_PKG_VERSION"),
        drawn
    );
    assert!(
        drawn.contains("GPL"),
        "the About dialog never names the licence: {}",
        drawn
    );
}

/// A released build must not open a console window behind the main window.
///
/// Windows decides that from the PE subsystem field: CONSOLE (3) makes the
/// loader allocate a console, GUI (2) does not. Rust emits CONSOLE unless
/// the crate asks otherwise, which is why 0.9.0 showed a black window on
/// every start. The attribute is checked here rather than the built binary
/// because the test binary is a console program by design -- only the
/// released executable is built with it.
#[test]
fn test_the_program_is_built_as_a_windows_program_not_a_console_one() {
    let main_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
        .expect("src/main.rs is where the crate attributes live");

    assert!(
        main_rs.contains("windows_subsystem = \"windows\""),
        "src/main.rs does not ask for the windows subsystem, so a release \
         build opens a console window on every start"
    );
    // The attribute must sit above everything else: an inner attribute
    // placed after an item is a compile error, so a careless move would
    // break the build -- but a cfg_attr that silently does not apply would
    // not, and that is what this guards.
    // At the start of a line, not anywhere: "use " also appears inside the
    // word "because", which is how this assertion first failed.
    let first_code = main_rs
        .find("\nuse ")
        .expect("main.rs has use declarations");
    let attribute = main_rs
        .find("windows_subsystem")
        .expect("checked just above");
    assert!(
        attribute < first_code,
        "the windows_subsystem attribute has to come before the first item"
    );
}
