<div align="center">
  <img src="assets/logo.png" width="96" alt="FARO QEMU MANAGER">
  <h1>FARO QEMU MANAGER</h1>
  <p><strong>A desktop manager for QEMU virtual machines on Windows, written in Rust.</strong></p>
</div>

QEMU is a superb emulator with a command line that nobody wants to retype.
FARO keeps your machines as configurations you edit in a window, builds the
command line for you, and stays out of the way while the guest runs.

## What it does

- **Machines as configurations.** Create, duplicate, and delete virtual
  machines; each one keeps its own folder, disks, and UEFI variable store.
- **A wizard with templates** for the usual guests, or direct mode if you
  would rather set every field yourself.
- **UEFI and BIOS**, with a per-VM variable store so a guest keeps its own
  boot entries.
- **Disks and drives:** qcow2 and raw images, SATA/VirtIO/IDE interfaces,
  boot order, CD/DVD ISOs.
- **Network:** user-mode NAT, bridged, or none; VirtIO/e1000/RTL8139 cards;
  port forwarding.
- **Display:** VirtIO, std, QXL, Cirrus, or headless; native, GTK, or SDL
  windows.
- **A console** that shows the guest's output and keeps it in a log file,
  with filtering and a line limit.
- **Clean shutdown over QMP** — the guest is asked to shut down, not killed,
  with forced stop as a deliberate second step.
- **Machines outlive the application.** Close FARO while a guest runs and the
  guest keeps running; start FARO again and it picks the machine back up,
  proving the machine's identity over QMP before it touches it.
- **A pre-flight check** before launch: missing images, missing firmware,
  more memory or cores than the host has, an accelerator this QEMU does not
  offer.
- **Three languages:** German, English, French.

## Download

Two shapes, both on the [releases page](https://github.com/RayTrunk/faro-qemu-manager/releases):

- **`FARO-QEMU-MANAGER-<version>-setup.msi`** — the installer. Puts the
  program in `Program Files`, adds a Start menu entry, and uninstalls
  through Windows' own list. Keeps its configuration in
  `%APPDATA%\faro-qem` and its machines in your user folder.
- **`FARO-QEMU-MANAGER-<version>-portable.zip`** — unpack and run. Because
  it carries a `portable.txt` next to the executable, *everything* it
  writes — machine list, settings, and the virtual machines themselves —
  stays in a `FARO-Data` folder beside it. Nothing touches the host's user
  profile, so the folder can live on a stick. Delete `portable.txt` and it
  behaves like an installed copy.

`SHA256SUMS.txt` on the release lets you verify what you downloaded.

Neither package contains QEMU. Install QEMU separately, or point FARO at a
QEMU folder in the settings.

## Requirements

- Windows 10 or 11 (the Windows resource and the WHPX accelerator path are
  Windows-specific; the code itself is portable).
- QEMU on `PATH` or in a directory you point the settings at.
  Developed and tested against **QEMU 11.1.0** from
  <https://qemu.weilnetz.de/w64/>.
- Rust 1.75 or newer to build it.

## Build

```
cargo build --release
```

The executable lands in `target/release/faro-qem.exe`. Run the tests with:

```
cargo test
```

## Status

**0.9.0.** The feature set above is implemented and covered by 366 automated
tests, including tests that start a real QEMU and talk to it over QMP.

It is not 1.0.0 for one reason: a Windows 11 installation has not yet been
walked through from start to finish in one sitting — wizard, UEFI, SATA, and
the real installer together. Everything that test would exercise is covered
in pieces, but the whole has not been signed off.

Two known limitations:

- **No single-instance lock.** A second copy of FARO will adopt the machines
  the first one started, and both will then act on the same guest.
- **No virtual TPM.** Windows 11 wants TPM 2.0 and Secure Boot, and swtpm
  needs Unix sockets, which QEMU cannot offer on Windows. The Windows
  template explains the `LabConfig` registry bypass the installer accepts.

## Licence

Copyright (C) R.Trunk 2026.

This program is free software: you can redistribute it and/or modify it
under the terms of the **GNU General Public License, version 3**, as
published by the Free Software Foundation.

This program is distributed in the hope that it will be useful, but WITHOUT
ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE. See [LICENSE](LICENSE) for the full text,
or <https://www.gnu.org/licenses/>.
