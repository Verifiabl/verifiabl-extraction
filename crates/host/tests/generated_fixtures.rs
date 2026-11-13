//! The fixture generator's files read as fixtures/expected.json says for the
//! committed ones. Their deflated streams can differ by byte, so this checks
//! the reading, and the bytes only of the pixel fixtures, which have no stream.

mod common;

use std::collections::BTreeSet;
use std::process::Command;

use common::{abi, read, Expectation, TIMEOUT};
use serde_json::Value;
use verifiabl_extract_host::{ReadKind, Sandbox};

#[test]
fn generated_fixtures_read_as_expected() {
    let out = std::env::temp_dir().join(format!("vf-generated-fixtures-{}", std::process::id()));
    let status = Command::new(env!("CARGO_BIN_EXE_generate-fixtures"))
        .args(["--out", out.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    let generated: BTreeSet<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(generated.len(), 8);

    let sandbox = Sandbox::new(&read("dist/verifiabl-extract.wasm"), abi()).unwrap();
    let mut checked = BTreeSet::new();
    for expectation in Expectation::all() {
        if !generated.contains(&expectation.file) {
            continue;
        }
        let name = expectation.name();
        let bytes = std::fs::read(out.join(&expectation.file)).unwrap();
        if expectation.kind() == ReadKind::Pixels {
            let committed = read(&format!("fixtures/{}", expectation.file));
            assert!(bytes == committed, "{name} is not the committed file");
        }
        let outcome = sandbox.extract(expectation.input(&bytes), TIMEOUT).unwrap();
        assert_eq!(outcome.status, expectation.status, "{name}");
        if expectation.status == "ok" {
            assert_eq!(
                outcome.result,
                Some(Value::Object(expectation.result)),
                "{name}"
            );
        }
        checked.insert(expectation.file);
    }
    assert_eq!(checked, generated, "a generated file has no expectation");
    std::fs::remove_dir_all(out).unwrap();
}
