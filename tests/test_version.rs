// FARO QEMU MANAGER (C) R.Trunk 2026
// The version number lives in two files that no compiler compares.

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
