# Regression inputs

Each input here caused a fuzz target to fail. Each folder has the name of its target. The fuzzer runs these inputs first. `cargo test` in `fuzz/` runs these inputs and the fixtures through the check of each target.
