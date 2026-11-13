//! The interface in `abi/extraction-abi.json`, read at run time so the host
//! cannot fall out of step with it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Abi {
    pub abi_version: u32,
    pub memory: Memory,
    pub limits: Limits,
    pub pixel_formats: BTreeMap<String, u32>,
    pub exports: BTreeMap<String, serde_json::Value>,
    pub status: BTreeMap<String, u32>,
    pub host_status: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub export: String,
    pub max_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_pixel_bytes: usize,
    pub max_pixels: u32,
    pub max_pdf_pages: u32,
    pub max_result_bytes: usize,
    pub max_payload_length: usize,
}

impl Abi {
    pub fn read(path: &Path) -> Result<Abi, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        serde_json::from_str(&text)
            .map_err(|error| format!("{} is not the interface: {error}", path.display()))
    }

    /// The name of a status code, or `None` for a code the interface does not define.
    pub fn status_name(&self, code: u32) -> Option<&str> {
        self.status
            .iter()
            .find(|(_, value)| **value == code)
            .map(|(name, _)| name.as_str())
    }
}
