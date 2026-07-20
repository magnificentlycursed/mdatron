# Red-gate retrofit evidence — issue #52 (2026-07-19)

```
=== RED-GATE RETROFIT EVIDENCE — issue #52 — 2026-07-20T00:18:38Z ===
Seed: predecessor confine_under_root from 0b21986^ restored into index.rs at HEAD (2122a80 + d13c798)

--- Run 1: full mdatron-core suite against seeded predecessor (in-tree tests, env::temp_dir roots = macOS divergence live) ---
test dsl::index::tests::deep_parent_traversal_with_nonexistent_target_is_rejected ... ok
test dsl::index::tests::absolute_source_with_nonexistent_target_rejected ... FAILED
test dsl::index::tests::confined_but_missing_source_is_io_not_traversal ... FAILED
test dsl::index::tests::absolute_source_rejected_even_when_inside_root ... FAILED
test dsl::index::tests::path_traversal_via_parent_dots_is_rejected ... ok
test dsl::index::tests::interior_parent_segment_rejected_even_when_non_escaping ... ok
test dsl::index::tests::glob_matched_symlink_refused ... FAILED
test dsl::index::tests::parent_traversal_rejected_when_target_exists ... ok
test dsl::index::tests::glob_pattern_with_parent_segment_is_rejected ... FAILED
test dsl::index::tests::symlink_source_refused_even_when_target_is_inside_root ... FAILED
test dsl::index::tests::symlink_source_pointing_outside_root_refused ... FAILED
test dsl::index::tests::symlinked_intermediate_component_refused ... FAILED
test result: FAILED. 160 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

--- Run 2 detail: retrofit suite (CANONICAL temp roots — divergence removed) against seeded predecessor ---
test rg_absolute_source_absent_target_rejected ... ok
test rg_control_confined_missing_source_errors_without_traversal_claim ... ok
test rg_parent_traversal_absent_target_rejected_as_confinement ... FAILED
test rg_deep_parent_traversal_absent_target_rejected ... FAILED
test rg_control_plain_read_succeeds ... ok
test rg_absolute_source_inside_root_rejected ... FAILED
test rg_interior_parent_segment_rejected_even_when_non_escaping ... FAILED
test rg_parent_traversal_existent_target_rejected ... ok
test rg_symlink_source_inside_root_refused ... FAILED
test rg_symlink_source_outside_root_refused ... ok
test rg_glob_matched_symlink_refused ... ok
test rg_glob_pattern_with_parent_segment_rejected ... FAILED
test rg_symlinked_intermediate_component_refused ... ok
thread 'rg_parent_traversal_absent_target_rejected_as_confinement' panicked at mdatron-core/tests/red_gate_retrofit.rs:64:18:
thread 'rg_deep_parent_traversal_absent_target_rejected' panicked at mdatron-core/tests/red_gate_retrofit.rs:78:18:
thread 'rg_absolute_source_inside_root_rejected' panicked at mdatron-core/tests/red_gate_retrofit.rs:143:18:
thread 'rg_interior_parent_segment_rejected_even_when_non_escaping' panicked at mdatron-core/tests/red_gate_retrofit.rs:111:18:
thread 'rg_symlink_source_inside_root_refused' panicked at mdatron-core/tests/red_gate_retrofit.rs:174:18:
thread 'rg_glob_pattern_with_parent_segment_rejected' panicked at mdatron-core/tests/red_gate_retrofit.rs:126:18:
test result: FAILED. 7 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```
