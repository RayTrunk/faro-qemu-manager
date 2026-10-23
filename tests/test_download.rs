// FARO QEMU MANAGER (C) R.Trunk 2026
// Downloading an installation image, and proving it arrived intact.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;

use faro_qem::download::sha256_of;

// --------------------------------------------------------------------------
// Task 1: the digest
// --------------------------------------------------------------------------

#[test]
fn test_sha256_matches_a_known_value() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("x");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(
        sha256_of(&path, &AtomicBool::new(false)).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn test_sha256_of_an_empty_file_is_the_empty_digest() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty");
    std::fs::write(&path, b"").unwrap();
    assert_eq!(
        sha256_of(&path, &AtomicBool::new(false)).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_sha256_stops_when_asked_to() {
    // A 6 GB image must not hold up the application's exit.
    let dir = tempdir().unwrap();
    let path = dir.path().join("big");
    std::fs::write(&path, vec![7u8; 8 * 1024 * 1024]).unwrap();
    let err = sha256_of(&path, &AtomicBool::new(true))
        .expect_err("a cancelled digest must not pretend to be a digest");
    assert!(!err.is_empty());
}

#[test]
fn test_sha256_of_a_missing_file_names_the_path() {
    let err = sha256_of(Path::new(r"C:\nope\x.iso"), &AtomicBool::new(false)).unwrap_err();
    assert!(err.contains("x.iso"), "got: {}", err);
}
