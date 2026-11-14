# DISINTEGRATOR v1.0.1

**Premium ultra-deep uninstaller for Windows 11** - the hardened release of
the forensic-grade purge console.

---

## Highlights

- **Registry Hunter** - budgeted BFS traversal of `HKLM/HKCU\SOFTWARE`,
  `WOW6432Node`, `HKCR` (CLSID, ProgID, shell extensions, file associations)
  and `HKLM\SYSTEM\CurrentControlSet\Services`. Keys that belong exclusively
  to the target are dissolved as a whole, individual values are scrubbed
  surgically.
- **Filesystem Obliteration** - recursive deletion of the payload tree with
  junction/symlink unlinking (never followed), attribute clearing, ownership
  takeover and `MoveFileExW(MOVEFILE_DELAY_UNTIL_REBOOT)` deferral for locked
  files.
- **Process Neutralisation** - Toolhelp32 snapshot with full process-tree
  resolution; children are terminated before their supervising parent so they
  cannot be respawned mid-teardown.
- **Service & Driver Scrub** - language independent discovery through the
  `Services` registry hive, SCM-driven stop and deletion.
- **Task Scheduler Purge** - scheduled tasks whose actions reference the
  target are unregistered through the Task Scheduler.
- **Autostart Strip** - `Run`/`RunOnce` values in both hives and the two
  startup folders.
- **MSIX / AppX support** - Store packages are inventoried and deregistered
  through the package manager.

## Interface refresh

The console was rebuilt around a softer, continuous geometry:

- every angular bevel (`clip-path`) was replaced by continuous radii, pill
  shaped chips and rounded toggles;
- hard inset shadows became diffused layered light with a soft orange bloom;
- transitions now use a gentle easing curve (`cubic-bezier(0.33, 1, 0.68, 1)`)
  so panels, buttons and badges breathe instead of snapping;
- the scanline veil and the micro grid were thinned out and masked towards the
  centre of the viewport;
- the disintegration particles became slow, round and translucent instead of
  hard orange squares.

## Safety guarantees

- A single **safety guard** mediates every destructive call: system
  subtrees (`C:\Windows`, `WindowsApps`, `Common Files\Microsoft Shared`, the
  MSI cache), protected containers, ancestors of protected locations, shared
  runtimes (VC++, DirectX, .NET, Python) and critical Windows services are
  hard-denied.
- A **system restore point** is created through `SRSetRestorePointW` before the
  first destructive action (with a `Checkpoint-Computer` fallback).
- The *Disintegration Chamber* shows every artefact that will be touched -
  including the ones the guard protects - before anything is destroyed.
- The executable is stamped with `requireAdministrator`, so Windows always
  asks before elevation. The manifest is only applied to release builds, which
  keeps the test harness runnable by an unprivileged CI runner.

## Verification

- 34 Rust unit tests covering registry traversal, file locking, junction
  handling, service protection and the safety heuristics.
- 9 frontend unit tests.
- An end-to-end test that generates a **complete dummy application** on the
  live machine (payload tree, registry tree, installer registration,
  autostart hook, Windows service, scheduled task) and asserts that 100 % of
  the generated artefacts are dissolved by the production pipeline.

## Artefacts

| File | Description |
|---|---|
| `Disintegrator_v1.0.1_x64.exe` | Portable single file executable |
| `Disintegrator_v1.0.1_x64-setup.exe` | NSIS installer (Start Menu and desktop shortcut) |
| `SHA256SUMS.txt` | Cryptographic checksums of every artefact |

## CLI

```powershell
disintegrator.exe --scan
disintegrator.exe --analyze "Contoso Suite"
disintegrator.exe --purge "Contoso Suite" --no-restore-point
```

---

**Full changelog**: see the repository history.
**License**: Apache-2.0 - (c) 2026 IBranch029.
