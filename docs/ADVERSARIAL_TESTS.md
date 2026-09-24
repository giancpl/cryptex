# Filesystem and recovery adversarial evidence

J2 exercises the filesystem boundary without running TeX or depending on ambient
developer state. These tests run in temporary directories as part of
`pnpm rust:test`.

| Threat                                     | Evidence                                                                                                                           |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| Absolute paths and traversal               | `project::tests::accepts_root_and_normal_relative_paths`; `recovery::tests::rejects_unsafe_paths_and_separates_project_identities` |
| Binary, invalid UTF-8, and oversized input | `project::tests::rejects_binary_invalid_utf8_and_oversized_files`                                                                  |
| Failed write preserving prior content      | `project::tests::rejected_oversized_write_preserves_the_original_file`                                                             |
| External edit before save                  | `project::tests::stale_fingerprint_never_overwrites_an_external_change`                                                            |
| Concurrent writers with one fingerprint    | `project::tests::concurrent_writes_with_one_fingerprint_have_one_winner`                                                           |
| Permission preservation                    | `project::tests::atomic_write_preserves_unix_permissions`                                                                          |
| Outside-root symlink                       | `project::tests::refuses_resolution_and_marks_symlinks_outside_root_inaccessible`                                                  |
| Symlink substituted after read             | `project::tests::symlink_swap_after_read_cannot_overwrite_an_outside_file`                                                         |
| Watcher event outside root                 | `watcher::tests::watcher_drops_paths_outside_the_project_root`                                                                     |
| Watcher backend overflow/failure           | `watcher::tests::watcher_backend_failure_requests_a_full_rescan`                                                                   |
| Self-write correlation                     | `watcher::tests::correlates_expected_write_by_final_fingerprint`                                                                   |
| Restart recovery lifecycle                 | `recovery::tests::snapshots_round_trip_and_delete_without_source_files`                                                            |
| Corrupt recovery data                      | `recovery::tests::corrupt_snapshots_are_reported_without_hiding_valid_ones`                                                        |
| Recovery storage permissions               | `recovery::tests::recovery_storage_uses_restrictive_permissions`                                                                   |

Project writes are serialized inside `ProjectService`. Fingerprint comparison and
atomic replacement happen while that guard is held, so two callers presenting the
same old fingerprint cannot both succeed. This is process-local coordination;
external writers remain controlled by the second disk fingerprint check and every
filesystem operation remains fallible.

The symlink substitution test replaces a previously read source with an outside-root
link and verifies both rejection and preservation of the outside file. This is
defense evidence for the supported path flow, not a claim that portable pathname APIs
form a proven OS sandbox. J5 retains the separate Linux sandbox investigation.
