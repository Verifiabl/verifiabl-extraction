//! What the tests share: the interface, and the entries of fixtures/expected.json.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Map, Value};
use verifiabl_extract_host::{extraction_root, Abi, Input, PixelShape, ReadKind};

pub const TIMEOUT: Duration = Duration::from_secs(30);

/// One read of a fixture, and what every host must get.
#[derive(Deserialize)]
pub struct Expectation {
    pub file: String,
    read: String,
    /// The shape of a pixel fixture, which has no header to give it.
    pixels: Option<Shape>,
    pub status: String,
    /// The result of a read that gives `ok`.
    #[serde(flatten)]
    pub result: Map<String, Value>,
}

#[derive(Deserialize)]
struct Shape {
    width: u32,
    height: u32,
    format: String,
}

impl Expectation {
    pub fn all() -> Vec<Expectation> {
        serde_json::from_slice(&read("fixtures/expected.json")).unwrap()
    }

    pub fn kind(&self) -> ReadKind {
        ReadKind::parse(&self.read).unwrap()
    }

    /// The read of `bytes` that this expectation describes.
    pub fn input<'a>(&'a self, bytes: &'a [u8]) -> Input<'a> {
        match &self.pixels {
            Some(shape) => Input::Pixels(
                bytes,
                PixelShape {
                    width: shape.width,
                    height: shape.height,
                    format: &shape.format,
                },
            ),
            None => Input::of(self.kind(), bytes),
        }
    }

    /// The fixture and its read, for a failed assertion.
    pub fn name(&self) -> String {
        match &self.pixels {
            Some(shape) => format!(
                "{} ({}x{} {})",
                self.file, shape.width, shape.height, shape.format
            ),
            None => format!("{} ({})", self.file, self.read),
        }
    }
}

pub fn abi() -> Abi {
    Abi::read(&extraction_root().join("abi/extraction-abi.json")).unwrap()
}

/// A file in the extraction folder.
pub fn read(path: &str) -> Vec<u8> {
    let path = extraction_root().join(path);
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}
