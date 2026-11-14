# DISINTEGRATOR v1.0.2

**Premium ultra-deep uninstaller for Windows 11** - the refined release of the
forensic-grade purge console.

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

## What changed in v1.0.2

- **The wordmark is HAYDEE.** The header no longer shouts a product name: the
  six glyphs now spread across the entire width of the bar, filling every
  pixel that is left to them, with a warm bloom that intensifies on hover.
- **The filter icon is a real vector.** The magnifier was a hand drawn CSS
  construction and looked crooked at every zoom level; it is now an inline SVG
  with a perfectly round lens and a round capped handle.
- **The repository badges are clean.** The downloads counter pointed at a
  non-existent endpoint and rendered as a `404 badge not found`; it now uses
  the total-downloads route and every badge resolves.
- **One hidden dedication.** `Ctrl+Alt+H` inside the console reveals it for
  the attentive ones.

## Interface

The console keeps the soft, continuous geometry introduced in v1.0.1: rounded
radii instead of angular bevels, diffused layered light, gentle easing curves
and translucent round particles.

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
| `Disintegrator_v1.0.2_x64.exe` | Portable single file executable |
| `Disintegrator_v1.0.2_x64-setup.exe` | NSIS installer (Start Menu and desktop shortcut) |
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
