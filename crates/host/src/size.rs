//! What takes up a module's size: each section, the code size of each linked
//! library and the largest functions. Libraries are attributed from function
//! names, so only a module with a name section, such as `nix build
//! .#module-debug`, can be attributed. twiggy cannot parse this module.

use std::collections::BTreeMap;

use wasmparser::{BinaryReaderError, KnownCustom, Name, Parser, Payload, TypeRef};

pub struct FunctionSize {
    pub name: String,
    pub bytes: usize,
}

pub struct ModuleSizes {
    pub total: usize,
    /// Section names and sizes, in module order.
    pub sections: Vec<(String, usize)>,
    pub functions: Vec<FunctionSize>,
}

// Rust crates by the name a v0-mangled symbol gives them.
const RUST_CRATES: &[(&str, &[&str])] = &[
    ("verifiabl-extract", &["verifiabl_extract"]),
    (
        "Rust image decoders",
        &[
            "png",
            "zune_jpeg",
            "zune_core",
            "image_webp",
            "byteorder_lite",
            "quick_error",
        ],
    ),
    (
        "Rust inflate and XML",
        &[
            "miniz_oxide",
            "fdeflate",
            "flate2",
            "zlib_rs",
            "crc32fast",
            "simd_adler32",
            "adler2",
            "roxmltree",
            "memchr",
        ],
    ),
    (
        "Rust standard library",
        &["core", "alloc", "std", "compiler_builtins"],
    ),
];

// Fragments of C++ mangled names, checked in order: JBIG2 lives inside PDFium.
const CPP_LIBRARIES: &[(&str, &[&str])] = &[
    ("ZXing-C++", &["5ZXing"]),
    ("libc++", &["St3__2", "__cxxabi", "10__cxxabiv1"]),
    ("JBIG2", &["JBig2"]),
    (
        "PDFium",
        &[
            "CPDF", "CFX", "CFFL", "CPWL", "fxcrt", "fxge", "fxcodec", "pdfium", "CPSO",
            "CStretch", "CFX_",
        ],
    ),
];

// Prefixes of C function names, checked in order.
const C_LIBRARIES: &[(&str, &[&str])] = &[
    ("verifiabl-extract", &["vf_"]),
    ("PDFium", &["FPDF", "FX_", "FXSYS_", "FXMEM_"]),
    ("OpenJPEG", &["opj_"]),
    (
        "FreeType",
        &[
            "FT_",
            "ft_",
            "TT_",
            "tt_",
            "cff_",
            "cf2_",
            "t1_",
            "T1_",
            "t42_",
            "ps_",
            "af_",
            "pfr_",
            "sfnt_",
            "cid_",
            "psh_",
            "pshinter_",
            "gray_",
        ],
    ),
    (
        "libjpeg-turbo",
        &[
            "jpeg_", "jinit_", "jsimd_", "jcopy_", "jdiv_", "jround_", "jzero_",
        ],
    ),
    ("Little CMS", &["cms", "_cms"]),
    ("libpng", &["png_"]),
    (
        "zlib",
        &[
            "deflate", "inflate", "adler32", "crc32", "zcalloc", "zcfree", "_tr_",
        ],
    ),
    (
        "C library",
        &[
            "__", "mem", "str", "malloc", "calloc", "realloc", "free", "dlmalloc", "dlfree",
            "abort", "qsort", "bsearch",
        ],
    ),
];

/// The library a function belongs to, or "other" for a name no rule matches,
/// such as a static C function without a library prefix.
pub fn library_of(name: &str) -> &'static str {
    if name.starts_with("_R") {
        let Some(krate) = rust_crate(name) else {
            return "other";
        };
        return RUST_CRATES
            .iter()
            .find(|(_, crates)| crates.contains(&krate))
            .map_or("Rust, other crates", |(library, _)| library);
    }
    // A C++ name nests its namespaces and classes, so a fragment can be anywhere in it.
    let cpp = name.starts_with("_Z");
    let rules = if cpp { CPP_LIBRARIES } else { C_LIBRARIES };
    let matches = |fragment: &&str| {
        if cpp {
            name.contains(fragment)
        } else {
            name.starts_with(fragment)
        }
    };
    rules
        .iter()
        .find(|(_, fragments)| fragments.iter().any(matches))
        .map_or("other", |(library, _)| library)
}

// The crate that defines a v0-mangled Rust symbol: the first crate root
// (`C`, an optional `s<base62>_` disambiguator, then a length-prefixed name).
// A generic function names the crate it was instantiated in last, so the
// first root is the one that wrote it.
fn rust_crate(name: &str) -> Option<&str> {
    let bytes = name.as_bytes();
    let mut index = 2;
    while index < bytes.len() {
        if bytes[index] == b'C' {
            let mut cursor = index + 1;
            if bytes.get(cursor) == Some(&b's') {
                cursor += 1;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                {
                    cursor += 1;
                }
                if bytes.get(cursor) != Some(&b'_') {
                    index += 1;
                    continue;
                }
                cursor += 1;
            }
            let digits = bytes[cursor..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits > 0 {
                let length: usize = name[cursor..cursor + digits].parse().ok()?;
                let start = cursor + digits;
                return name.get(start..start + length);
            }
        }
        index += 1;
    }
    None
}

pub fn module_sizes(wasm: &[u8]) -> Result<ModuleSizes, BinaryReaderError> {
    let mut sections = Vec::new();
    let mut bodies = Vec::new();
    let mut names = BTreeMap::new();
    let mut imported_functions = 0u32;
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload?;
        match &payload {
            Payload::ImportSection(reader) => {
                for import in reader.clone().into_imports() {
                    if matches!(import?.ty, TypeRef::Func(_)) {
                        imported_functions += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let range = body.range();
                // Offsets within a module, which is far smaller than usize allows.
                bodies.push((range.end - range.start) as usize);
            }
            Payload::CustomSection(reader) => {
                if let KnownCustom::Name(name_reader) = reader.as_known() {
                    for name in name_reader {
                        if let Name::Function(map) = name? {
                            for naming in map {
                                let naming = naming?;
                                names.insert(naming.index, naming.name.to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some((id, range)) = payload.as_section() {
            let name = match &payload {
                Payload::CustomSection(reader) => format!("custom {}", reader.name()),
                _ => section_name(id).to_string(),
            };
            sections.push((name, (range.end - range.start) as usize));
        }
    }
    let functions = bodies
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| {
            let index = imported_functions + index as u32;
            FunctionSize {
                name: names
                    .remove(&index)
                    .unwrap_or_else(|| format!("function {index}")),
                bytes,
            }
        })
        .collect();
    Ok(ModuleSizes {
        total: wasm.len(),
        sections,
        functions,
    })
}

/// Code bytes per library, largest first.
pub fn sizes_by_library(functions: &[FunctionSize]) -> Vec<(&'static str, usize)> {
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();
    for function in functions {
        *totals.entry(library_of(&function.name)).or_default() += function.bytes;
    }
    let mut totals: Vec<_> = totals.into_iter().collect();
    totals.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    totals
}

fn section_name(id: u8) -> &'static str {
    match id {
        1 => "type",
        2 => "import",
        3 => "function",
        4 => "table",
        5 => "memory",
        6 => "global",
        7 => "export",
        8 => "start",
        9 => "element",
        10 => "code",
        11 => "data",
        12 => "data count",
        13 => "tag",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_functions_by_their_names() {
        let cases = [
            ("_ZN5ZXing10QRCode6Reader6decodeEv", "ZXing-C++"),
            ("_ZNSt3__212basic_stringIcEC2Ev", "libc++"),
            ("_ZN15CPDF_GenerateAP15GenerateAnnotAPEv", "PDFium"),
            ("FPDF_LoadMemDocument64", "PDFium"),
            ("opj_t1_dec_clnpass", "OpenJPEG"),
            ("TT_RunIns", "FreeType"),
            ("cf2_interpT2CharString", "FreeType"),
            ("_RNvNtCscaUv5THh8wD_17verifiabl_extract5image11decode_file", "verifiabl-extract"),
            // Instantiated in our crate, written in zune-jpeg.
            (
                "_RNvMNtCs7eOE1So6Ej9_9zune_jpeg8mcu_progINtNtB4_7decoder11JpegDecoderINtNtNtCs1JaIxvBgNiK_4core2io6cursor6CursorRShEE24parse_entropy_coded_dataCscaUv5THh8wD_17verifiabl_extract",
                "Rust image decoders",
            ),
            ("_RNvNtNtCs76Gn3FSQDkT_11miniz_oxide7inflate4core10decompress", "Rust inflate and XML"),
            ("_RNvMNtCs1JaIxvBgNiK_4core3fmtNtB2_9Formatter3pad", "Rust standard library"),
            ("vf_read_pdf", "verifiabl-extract"),
            ("static_helper", "other"),
        ];
        for (name, library) in cases {
            assert_eq!(library_of(name), library, "{name}");
        }
    }

    #[test]
    fn totals_each_library_largest_first() {
        let functions = [
            FunctionSize {
                name: "FPDF_A".to_string(),
                bytes: 10,
            },
            FunctionSize {
                name: "_ZN5ZXing1AEv".to_string(),
                bytes: 30,
            },
            FunctionSize {
                name: "FPDF_B".to_string(),
                bytes: 5,
            },
        ];
        assert_eq!(
            sizes_by_library(&functions),
            vec![("ZXing-C++", 30), ("PDFium", 15)]
        );
    }
}
