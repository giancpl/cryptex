# Acceptance

A task is complete only when its documented behavior and ADRs are satisfied,
types/lints/tests pass, security implications are reviewed, failures and races are
covered where relevant, documentation is current, and standard LaTeX portability
is preserved.

V0.1 additionally requires clean-machine Linux installation, offline fixture
compilation, no known critical/high path/process/data-loss issue, explicit build
permissions, last-success PDF behavior, bidirectional SyncTeX, deterministic local
assistance, compatibility with ordinary `latexmk`, and complete license notices.

## J1 fixture harness

The checked-in manifest at `tests/acceptance/manifest.json` maps every V0.1
fixture named by the implementation plan to a deterministic mode:

- `compile`: must finish successfully and produce the declared PDF/SyncTeX files.
- `expectedFailure`: must fail and retain the declared diagnostic evidence.
- `static`: feeds file, parser, trust, race, viewer, or performance tests without
  executing TeX.

`pnpm acceptance:validate` checks that the full matrix, roots, auxiliary inputs,
modes, and expectations are present. It never discovers or invokes a system TeX
installation. `pnpm acceptance:compile -- /absolute/managed/tex/bin/platform`
copies every compilable fixture into an isolated temporary directory and invokes
only the explicitly supplied `latexmk`, with `-norc`, an argument array, a
bounded output buffer, a minimal environment, and no shell. The harness checks
semantic outcomes and required artifacts instead of comparing unstable complete
logs or PDF bytes.

The harness has regression tests proving that a removed matrix entry or altered
required outcome fails validation. J2-J6 extend these fixtures with adversarial,
performance, portability, packaging, and release-machine evidence; J1 does not
claim those later gates already pass.
