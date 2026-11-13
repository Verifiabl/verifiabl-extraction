//! Builds dist/THIRD-PARTY-NOTICES.txt: the licences of everything linked into
//! the module, read from what the build links, so the file cannot fall behind
//! a dependency. PDFium, ZXing-C++ and the Rust crates ship their licences;
//! licenses/ holds those of the C, C++ and Rust runtimes, which do not.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenceFile {
    pub label: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub files: Vec<LicenceFile>,
}

/// The versions of the toolchain whose runtimes licenses/ covers.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    pub wasi_sdk: String,
    pub wasi_libc: String,
    pub llvm: String,
    pub rustc: String,
}

/// Where the build's inputs are.
pub struct Inputs {
    /// `cargo metadata --format-version 1 --filter-platform wasm32-unknown-unknown`
    /// for the module's crate.
    pub cargo_metadata: PathBuf,
    pub pdfium: PathBuf,
    pub zxing: PathBuf,
    pub wasi_sdk: PathBuf,
    pub licenses: PathBuf,
    /// The output of `rustc --version`.
    pub rustc_version: String,
}

#[derive(Deserialize)]
struct StaticComponent {
    name: String,
    version: String,
    license: String,
    source: String,
    files: Vec<String>,
}

#[derive(Deserialize)]
struct ListedComponents {
    toolchain: Toolchain,
    components: Vec<StaticComponent>,
}

#[derive(Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    manifest_path: PathBuf,
    repository: Option<String>,
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Resolve {
    root: String,
    nodes: Vec<ResolveNode>,
}

#[derive(Deserialize)]
struct ResolveNode {
    id: String,
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn read_text(path: &Path) -> Result<String> {
    Ok(decode(&read(path)?))
}

// Licences are plain text in UTF-8, apart from older ones in Latin-1.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| char::from(*byte)).collect(),
    }
}

fn read_licence(path: &Path, label: &str) -> Result<LicenceFile> {
    let text = read_text(path)?.replace("\r\n", "\n");
    // A byte order mark counts as white space, so a licence file that starts
    // with one prints the same as one that does not.
    let text = text
        .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .to_string();
    if text.is_empty() {
        return Err(format!("The licence file {label} is empty."));
    }
    Ok(LicenceFile {
        label: label.to_string(),
        text,
    })
}

fn sorted_names(folder: &Path) -> Result<Vec<String>> {
    let mut names = fs::read_dir(folder)
        .map_err(|error| format!("cannot list {}: {error}", folder.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .map_err(|error| format!("cannot list {}: {error}", folder.display()))
        })
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

fn is_licence_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "licence",
        "license",
        "copying",
        "copyright",
        "notice",
        "unlicense",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// The licence files at the top of a folder, in name order.
pub fn licence_files(folder: &Path) -> Result<Vec<LicenceFile>> {
    sorted_names(folder)?
        .into_iter()
        .filter(|name| is_licence_name(name) && folder.join(name).is_file())
        .map(|name| read_licence(&folder.join(&name), &name))
        .collect()
}

fn pdfium_version(pdfium: &Path) -> Result<String> {
    let text = read_text(&pdfium.join("VERSION"))?;
    let field = |key: &str| {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| *name == key)
            .map_or("?".to_string(), |(_, value)| value.trim().to_string())
    };
    Ok(["MAJOR", "MINOR", "BUILD", "PATCH"].map(field).join("."))
}

fn pdfium_component(pdfium: &Path) -> Result<Component> {
    let bundled = pdfium.join("licenses");
    let mut files = sorted_names(&bundled)?
        .into_iter()
        .map(|name| read_licence(&bundled.join(&name), &format!("licenses/{name}")))
        .collect::<Result<Vec<_>>>()?;
    files.push(read_licence(
        &pdfium.join("LICENSE"),
        "LICENSE of the WASI port",
    )?);
    Ok(Component {
        name: "PDFium, and the libraries built into it".to_string(),
        version: pdfium_version(pdfium)?,
        license: "PDFium's licence, the bundled libraries' licences and the WASI port's".to_string(),
        source: "https://pdfium.googlesource.com/pdfium, with the WASI port from https://github.com/run-llama/pdfium-binaries".to_string(),
        files,
    })
}

fn source_files(folder: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for name in sorted_names(folder)? {
        let path = folder.join(&name);
        if path.is_dir() {
            source_files(&path, files)?;
        } else if [".h", ".c", ".cpp"].iter().any(|ext| name.ends_with(ext)) {
            files.push(path);
        }
    }
    Ok(())
}

// The whole expression after the first SPDX identifier, so "Apache-2.0 OR
// MIT" is not taken for Apache-2.0.
fn spdx_expression(source: &str) -> Option<String> {
    const MARKER: &str = "SPDX-License-Identifier:";
    let start = source.find(MARKER)? + MARKER.len();
    let line = source[start..].split('\n').next().unwrap_or_default();
    let line = line.trim_end();
    let line = line.strip_suffix("*/").unwrap_or(line);
    Some(line.trim().to_string())
}

// ZXing-C++ states each file's licence in the file. Its own LICENSE covers
// Apache-2.0; licenses/ covers libzueci; libzint is only built with writers.
fn zxing_licence_listed(expression: &str, file: &str) -> bool {
    match expression {
        "Apache-2.0" => true,
        "BSD-3-Clause" => file.starts_with("libzueci/") || file.starts_with("libzint/"),
        _ => false,
    }
}

/// Source files whose stated licence the notices do not carry.
pub fn unlisted_zxing_licences(sources: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    source_files(sources, &mut files)?;
    let mut unlisted = Vec::new();
    for path in files {
        let file = path
            .strip_prefix(sources)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let Some(expression) = spdx_expression(&read_text(&path)?) else {
            continue;
        };
        if !zxing_licence_listed(&expression, &file) {
            unlisted.push(format!("{file}: {expression}"));
        }
    }
    unlisted.sort();
    Ok(unlisted)
}

fn zxing_version(cmake: &str) -> Option<String> {
    const MARKER: &str = "(ZXing VERSION \"";
    let start = cmake.find(MARKER)? + MARKER.len();
    let end = cmake[start..].find('"')?;
    Some(cmake[start..start + end].to_string())
}

fn zxing_component(zxing: &Path) -> Result<Component> {
    let unlisted = unlisted_zxing_licences(&zxing.join("core/src"))?;
    if !unlisted.is_empty() {
        return Err(format!(
            "ZXing-C++ has source under a licence the notices do not carry; add it to licenses/:\n{}",
            unlisted.join("\n")
        ));
    }
    let version = zxing_version(&read_text(&zxing.join("core/CMakeLists.txt"))?)
        .ok_or("ZXing-C++'s version is not in core/CMakeLists.txt.")?;
    Ok(Component {
        name: "ZXing-C++".to_string(),
        version,
        license: "Apache-2.0".to_string(),
        source: "https://github.com/zxing-cpp/zxing-cpp".to_string(),
        files: vec![read_licence(&zxing.join("LICENSE"), "LICENSE")?],
    })
}

fn first_word_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|rest| rest.split_whitespace().next())
}

fn actual_toolchain(wasi_sdk: &Path, rustc_version: &str) -> Result<Toolchain> {
    let version = read_text(&wasi_sdk.join("VERSION"))?;
    let missing = |what: &str| format!("Could not read {what}.");
    Ok(Toolchain {
        wasi_sdk: version
            .split_whitespace()
            .next()
            .ok_or_else(|| missing("wasi-sdk's version"))?
            .to_string(),
        wasi_libc: first_word_after(&version, "wasi-libc: ")
            .ok_or_else(|| missing("wasi-sdk's wasi-libc commit"))?
            .to_string(),
        llvm: first_word_after(&version, "llvm: ")
            .ok_or_else(|| missing("wasi-sdk's LLVM commit"))?
            .to_string(),
        rustc: first_word_after(rustc_version, "rustc ")
            .ok_or_else(|| missing("rustc's version"))?
            .to_string(),
    })
}

/// Why licenses/ does not describe the toolchain in use, or None when it does.
pub fn toolchain_problem(listed: &Toolchain, actual: &Toolchain) -> Option<String> {
    let fields = [
        ("wasiSdk", &listed.wasi_sdk, &actual.wasi_sdk),
        ("wasiLibc", &listed.wasi_libc, &actual.wasi_libc),
        ("llvm", &listed.llvm, &actual.llvm),
        ("rustc", &listed.rustc, &actual.rustc),
    ];
    let lines: Vec<String> = fields
        .iter()
        .filter(|(_, listed, actual)| listed != actual)
        .map(|(key, listed, actual)| {
            format!("{key}: licenses/ is for {listed}, the build uses {actual}")
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "licenses/ holds the licences of another toolchain; replace them from the new versions and update licenses/components.json:\n{}",
        lines.join("\n")
    ))
}

fn toolchain_components(inputs: &Inputs) -> Result<Vec<Component>> {
    let listed: ListedComponents =
        serde_json::from_slice(&read(&inputs.licenses.join("components.json"))?)
            .map_err(|error| format!("licenses/components.json: {error}"))?;
    let actual = actual_toolchain(&inputs.wasi_sdk, &inputs.rustc_version)?;
    if let Some(problem) = toolchain_problem(&listed.toolchain, &actual) {
        return Err(problem);
    }
    listed
        .components
        .into_iter()
        .map(|component| {
            let files = component
                .files
                .iter()
                .map(|file| read_licence(&inputs.licenses.join(file), file))
                .collect::<Result<Vec<_>>>()?;
            Ok(Component {
                name: component.name,
                version: component.version,
                license: component.license,
                source: component.source,
                files,
            })
        })
        .collect()
}

fn crate_component(package: CargoPackage) -> Result<Component> {
    let folder = package
        .manifest_path
        .parent()
        .ok_or_else(|| format!("{} has no folder", package.manifest_path.display()))?;
    let files = licence_files(folder)?;
    if files.is_empty() {
        return Err(format!(
            "The crate {} {} ships no licence file; add its licence to licenses/.",
            package.name, package.version
        ));
    }
    Ok(Component {
        name: format!("{} (Rust crate)", package.name),
        version: package.version,
        license: package
            .license
            .unwrap_or_else(|| "see its licence files".to_string()),
        source: package
            .repository
            .unwrap_or_else(|| format!("https://crates.io/crates/{}", package.name)),
        files,
    })
}

// Every crate the module links, apart from the module's own.
fn crate_components(metadata: &Path) -> Result<Vec<Component>> {
    let metadata: CargoMetadata = serde_json::from_slice(&read(metadata)?)
        .map_err(|error| format!("{} is not cargo metadata: {error}", metadata.display()))?;
    let linked: HashSet<String> = metadata
        .resolve
        .nodes
        .into_iter()
        .map(|node| node.id)
        .collect();
    let root = metadata.resolve.root;
    let mut packages: Vec<CargoPackage> = metadata
        .packages
        .into_iter()
        .filter(|package| linked.contains(&package.id) && package.id != root)
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));
    packages.into_iter().map(crate_component).collect()
}

pub fn linked_components(inputs: &Inputs) -> Result<Vec<Component>> {
    let mut components = vec![
        pdfium_component(&inputs.pdfium)?,
        zxing_component(&inputs.zxing)?,
    ];
    components.extend(toolchain_components(inputs)?);
    components.extend(crate_components(&inputs.cargo_metadata)?);
    Ok(components)
}

const RULE_WIDTH: usize = 78;

/// The notices file: each component with the numbers of its licence texts,
/// then every text once. Many components share one text, such as Apache-2.0.
pub fn render_notices(components: &[Component]) -> Result<String> {
    let mut texts: Vec<&str> = Vec::new();
    let mut sections = Vec::new();
    for component in components {
        if component.files.is_empty() {
            return Err(format!("{} has no licence text.", component.name));
        }
        let mut lines = vec![
            format!("{} {}", component.name, component.version),
            format!("  Licence: {}", component.license),
            format!("  Source: {}", component.source),
        ];
        for file in &component.files {
            let number = match texts.iter().position(|text| *text == file.text) {
                Some(index) => index + 1,
                None => {
                    texts.push(&file.text);
                    texts.len()
                }
            };
            lines.push(format!("  {}: text {number}", file.label));
        }
        sections.push(lines.join("\n"));
    }
    let rule = "=".repeat(RULE_WIDTH);
    let numbered: Vec<String> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| format!("{rule}\nText {}\n{rule}\n\n{text}", index + 1))
        .collect();
    Ok([
        "Third-party notices for verifiabl-extract.wasm",
        "",
        "The module contains code from the projects below. Each is listed with the",
        "files that state its licence; the texts follow, each printed once.",
        "",
        &sections.join("\n\n"),
        "",
        &numbered.join("\n\n"),
        "",
    ]
    .join("\n"))
}

pub fn third_party_notices(inputs: &Inputs) -> Result<String> {
    render_notices(&linked_components(inputs)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toolchain() -> Toolchain {
        Toolchain {
            wasi_sdk: "34.0".to_string(),
            wasi_libc: "2e6fb9d8ee0c".to_string(),
            llvm: "895aa2c896ad".to_string(),
            rustc: "1.99.0".to_string(),
        }
    }

    fn component(name: &str, texts: &[&str]) -> Component {
        Component {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            license: "MIT".to_string(),
            source: "https://example.com/source".to_string(),
            files: texts
                .iter()
                .enumerate()
                .map(|(index, text)| LicenceFile {
                    label: format!("LICENSE-{index}"),
                    text: text.to_string(),
                })
                .collect(),
        }
    }

    // A folder of files, removed when the guard drops.
    struct Folder(PathBuf);

    impl Folder {
        fn new(test: &str, files: &[(&str, &str)]) -> Folder {
            let root =
                std::env::temp_dir().join(format!("vf-notices-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            for (name, content) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
            fs::create_dir_all(&root).unwrap();
            Folder(root)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_licence_text_that_several_components_share_is_printed_once() {
        let notices = render_notices(&[
            component("first", &["shared text", "first's own"]),
            component("second", &["shared text"]),
        ])
        .unwrap();
        assert_eq!(notices.matches("shared text").count(), 1);
        assert!(notices.contains(
            "first 1.0.0\n  Licence: MIT\n  Source: https://example.com/source\n  LICENSE-0: text 1\n  LICENSE-1: text 2"
        ));
        assert!(notices.contains("second 1.0.0\n  Licence: MIT\n  Source: https://example.com/source\n  LICENSE-0: text 1"));
        assert!(notices.contains(&format!(
            "Text 2\n{}\n\nfirst's own",
            "=".repeat(RULE_WIDTH)
        )));
    }

    #[test]
    fn a_component_without_a_licence_text_is_refused() {
        assert_eq!(
            render_notices(&[component("bare", &[])]),
            Err("bare has no licence text.".to_string())
        );
    }

    #[test]
    fn licence_files_are_found_by_name_whatever_their_case_or_extension() {
        let crate_folder = Folder::new(
            "crate",
            &[
                ("LICENSE-MIT", "mit\n"),
                ("license-apache.md", "apache\r\n"),
                ("COPYING", "copying"),
                ("src/LICENSE", "not at the top"),
                ("README.md", "readme"),
            ],
        );
        let labels: Vec<(String, String)> = licence_files(&crate_folder.0)
            .unwrap()
            .into_iter()
            .map(|file| (file.label, file.text))
            .collect();
        assert_eq!(
            labels,
            [
                ("COPYING", "copying"),
                ("LICENSE-MIT", "mit"),
                ("license-apache.md", "apache")
            ]
            .map(|(label, text)| (label.to_string(), text.to_string()))
        );
        let readme_only = Folder::new("readme", &[("README.md", "readme")]);
        assert_eq!(licence_files(&readme_only.0).unwrap(), Vec::new());
    }

    #[test]
    fn licences_kept_for_another_toolchain_are_refused() {
        assert_eq!(toolchain_problem(&toolchain(), &toolchain()), None);
        let actual = Toolchain {
            wasi_sdk: "35.0".to_string(),
            rustc: "2.0.0".to_string(),
            ..toolchain()
        };
        let problem = toolchain_problem(&toolchain(), &actual).unwrap();
        assert!(problem.contains("wasiSdk: licenses/ is for 34.0, the build uses 35.0"));
        assert!(problem.contains("rustc: licenses/ is for 1.99.0, the build uses 2.0.0"));
        assert!(!problem.contains("llvm"));
    }

    #[test]
    fn zxing_source_under_a_licence_without_a_notice_is_named() {
        let sources = Folder::new(
            "zxing",
            &[
                (
                    "qrcode/Reader.cpp",
                    "// SPDX-License-Identifier: Apache-2.0\n",
                ),
                (
                    "libzueci/zueci.c",
                    "/* SPDX-License-Identifier: BSD-3-Clause */\n",
                ),
                (
                    "aztec/Borrowed.h",
                    "// SPDX-License-Identifier: BSD-3-Clause\n",
                ),
                ("oned/Other.cpp", "// SPDX-License-Identifier: MPL-2.0\n"),
                (
                    "pdf417/Either.cpp",
                    "// SPDX-License-Identifier: Apache-2.0 OR MIT\n#include <vector>\n",
                ),
                ("Plain.h", "// no identifier\n"),
            ],
        );
        assert_eq!(
            unlisted_zxing_licences(&sources.0).unwrap(),
            [
                "aztec/Borrowed.h: BSD-3-Clause",
                "oned/Other.cpp: MPL-2.0",
                "pdf417/Either.cpp: Apache-2.0 OR MIT",
            ]
        );
    }
}
