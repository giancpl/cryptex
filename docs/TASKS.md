# Ordered tasks

The authoritative sequence is:

1. A1 specifications and ADRs.
2. A2 Tauri/React/TypeScript bootstrap.
3. A3 checks and tests.
4. A4 typed backend boundary.
5. A5 CI and dependency controls.
6. D1 managed TeX packaging spike.
7. B1 safe project identity and paths.
8. B2 bounded file tree.
9. B3 fingerprinted reads and atomic writes.
10. C1 CodeMirror and document tabs.

Continue with C2-C6, D2-D4, E1-E4, F1-F6, G1-G4, H1-H5, I1-I5, and J1-J6 in
dependency order. M8-M10 remain blocked until J6. Each task is an independently
reviewable change with tests and the global acceptance rules.
