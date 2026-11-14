# DISINTEGRATOR v1.0.4

**Premium ultra-deep uninstaller for Windows 11** - the release that gives the
console real depth instead of flat paint.

---

## What changed in v1.0.4

### Matte depth, not flat paint

The whole interface was rebuilt around a physical depth system, in the spirit
of ExLoader: layered, quietly luxurious instrumentation that you can *feel*:

- **four elevation levels** (`--elev-1 … --elev-4`) stack a tight contact
  shadow under a wide ambient shadow and add a top inner highlight - panels,
  cards, chips and modals now read as lifted objects instead of rectangles;
- **recessed wells** (`--well`) sink the search field, the segmented control,
  the gauge tracks and the toggles into the chassis, which is what gives the
  surrounding surfaces something to sit against;
- **matte surfaces**: six warm charcoal tones replace the old flat greys, the
  glass blur is gone and a barely visible grain finishes the finish so the
  charcoal never looks like plastic;
- a **studio light** falls from the upper left with a cool fill from the
  lower right, and every panel header carries a seam of contact shadow;
- the **HAYDEE** wordmark is embossed (dark contact shadow below, light edge
  above) and the primary action is a raised plate that visibly presses into
  the surface when clicked;
- rows, residue cards and window buttons **lift by one pixel** on hover and
  cast a deeper shadow, so the interface reacts like a physical console.

### Still in place from v1.0.3

- the console is **frameless** with its own 44 px instrument strip, live
  context readout and hand drawn window controls;
- the **HAYDEE** wordmark spreads across the full width of the header;
- the filter magnifier is a real vector with a perfectly round lens;
- every repository badge resolves - no more `404 badge not found`;
- `Ctrl+Alt+H` reveals the hidden dedication for the attentive ones.

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
| `Disintegrator_v1.0.4_x64.exe` | Portable single file executable |
| `Disintegrator_v1.0.4_x64-setup.exe` | NSIS installer (Start Menu and desktop shortcut) |
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
