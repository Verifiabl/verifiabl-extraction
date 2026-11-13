//! The checks of each fuzz target. The targets in `fuzz_targets/` give them
//! inputs from libFuzzer. The tests in `tests/` give them the kept inputs, so
//! each change runs those inputs without a libFuzzer build.

pub mod image;
pub mod metadata;
pub mod metadata_shape;
pub mod result;

/// A fuzz target's name, as `fuzz_targets/` and `regressions/` use it, and its check.
pub struct Target {
    pub name: &'static str,
    pub check: fn(&[u8]),
}

pub const TARGETS: &[Target] = &[
    Target {
        name: "metadata",
        check: metadata::check,
    },
    Target {
        name: "metadata_shape",
        check: metadata_shape::check,
    },
    Target {
        name: "image",
        check: image::check,
    },
    Target {
        name: "result",
        check: result::check,
    },
];
