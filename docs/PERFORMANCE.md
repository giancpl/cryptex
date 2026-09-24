# Performance and stability budgets

J3 defines release budgets as executable tests. The regular suite compiles the
benchmark but leaves the filesystem-heavy test ignored. Run all budgets with:

```bash
pnpm performance:test
```

The weekly and manually dispatched `Performance budgets` workflow runs the same
command in release mode and uploads its complete log. A regression therefore remains
visible without making every feature pull request depend on shared-runner timing.

| Scenario                                   |    Budget | Baseline measured 2026-09-24 |
| ------------------------------------------ | --------: | ---------------------------: |
| Cold React workspace shell render          |  2,000 ms |                       157 ms |
| Lazy root listing with 10,000 files        |  3,000 ms |                        23 ms |
| Full best-effort index of 10,000 files     | 20,000 ms |                       134 ms |
| Tolerant scan of a 5 MiB source            |  3,000 ms |                        12 ms |
| Scanner RSS increase on Linux              |   256 MiB |               0 KiB observed |
| 5,000 deterministic Finder queries         |  3,000 ms |                     1,130 ms |
| 100 fingerprinted atomic read/write cycles |  8,000 ms |                         3 ms |
| 100 retained build/view artifact cycles    |  8,000 ms |                         5 ms |

The baseline excludes fixture construction so it measures application work rather
than test-data generation. Budgets are deliberately much wider than this workstation's
measurements to accommodate shared CI variance while still exposing algorithmic or I/O
regressions.

The tree assertion also verifies bounded lazy behavior: only 5,000 sorted entries are
returned and the result is marked truncated. The project index must still publish all
10,000 admitted sources. The source benchmark uses the configured 5 MiB boundary.
Finder ranking uses the bundled catalog and a real 10,000-file index. Artifact cycling
performs atomic publication, validation, bounded reads, and unique operation IDs; it
does not claim to measure TeX engine runtime.

Desktop process startup, GPU/PDF rendering memory, and real managed-TeX compilation
remain clean-machine/package measurements for J5-J6. Their absence here is explicit:
the J3 tests cover deterministic repository-owned work and never fall back to system
TeX or require a display server.
