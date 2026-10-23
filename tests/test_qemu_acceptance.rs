// FARO QEMU MANAGER (C) R.Trunk 2026
// Does the installed QEMU actually accept the command lines we build?

mod common;

use common::{qemu_accepts, Acceptance};
use faro_qem::config::Arch;

#[test]
fn test_the_probe_can_tell_good_arguments_from_bad() {
    match qemu_accepts(Arch::X86_64, &["-machine".into(), "q35".into()]) {
        Acceptance::NoQemu => eprintln!("skipped: no qemu-system-x86_64 on this machine"),
        Acceptance::Accepted => {}
        Acceptance::Rejected { first_line } => panic!("q35 must be accepted: {}", first_line),
    }

    // A bus type QEMU does not have: this is one of the eight bugs.
    match qemu_accepts(
        Arch::X86_64,
        &[
            "-machine".into(),
            "q35".into(),
            "-drive".into(),
            "if=sata,file=nonexistent.qcow2".into(),
        ],
    ) {
        Acceptance::NoQemu => {}
        Acceptance::Rejected { first_line } => assert!(
            first_line.contains("sata"),
            "the rejection must name the offending option, got: {}",
            first_line
        ),
        Acceptance::Accepted => panic!("if=sata must be rejected — the probe has no teeth"),
    }
}
