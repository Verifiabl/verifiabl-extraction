//! Reads a PDF's Verifiabl payload: the XMP metadata payload when there is one,
//! otherwise the QR codes on its pages, rendered with PDFium (libpdfium.a)
//! through native/pdfium_shim.cpp.

use std::ffi::c_void;

use crate::{metadata, qr, MAX_RESULT_BYTES};

/// A longer PDF without a metadata payload is refused, not read in part.
pub const MAX_PAGES: i32 = 50;
/// Enough for a QR badge of a few centimetres to have several pixels per module.
const RENDER_DPI: f32 = 200.0;
/// Pixels across all rendered pages; each page is also capped at the caller's maximum.
const MAX_TOTAL_PIXELS: u64 = 200_000_000;

/// `vf_pdf_failure` after a failed allocation.
const OUT_OF_MEMORY: i32 = -2;

type Handle = *mut c_void;

extern "C" {
    fn vf_pdf_failure() -> i32;
    fn vf_pdf_open(data: *const u8, size: usize) -> Handle;
    fn vf_pdf_page_count(document: Handle) -> i32;
    fn vf_pdf_load_page(document: Handle, index: i32) -> Handle;
    fn vf_pdf_page_width(page: Handle) -> f32;
    fn vf_pdf_page_height(page: Handle) -> f32;
    fn vf_pdf_render(page: Handle, width: i32, height: i32) -> Handle;
    fn vf_pdf_bitmap_rows(bitmap: Handle) -> *const u8;
    fn vf_pdf_bitmap_stride(bitmap: Handle) -> i32;
    fn vf_pdf_bitmap_destroy(bitmap: Handle);
    fn vf_pdf_close_page(page: Handle);
    fn vf_pdf_close(document: Handle);
}

pub struct PdfPayloads {
    /// Every `payload` value in the PDF's XMP metadata streams short enough to be a payload.
    pub metadata: Vec<String>,
    /// QR code texts from rendered pages; empty when there is a metadata payload.
    pub codes: Vec<String>,
    pub pages: i32,
}

pub enum PdfFailure {
    Unreadable,
    TooManyPages,
    TooMuchMetadata,
    LimitExceeded,
    ResultTooLarge,
}

pub fn read(file: &[u8], max_page_pixels: u64) -> Result<PdfPayloads, PdfFailure> {
    // The metadata payload is read first, and a PDF with one is never rendered.
    let metadata = metadata::payloads(file)
        .map_err(|metadata::TooMuchMetadata| PdfFailure::TooMuchMetadata)?;
    if !metadata.is_empty() {
        return Ok(PdfPayloads {
            metadata,
            codes: Vec::new(),
            pages: 0,
        });
    }

    let document = pdfium(|| unsafe { vf_pdf_open(file.as_ptr(), file.len()) })?;
    if document.is_null() {
        // Damaged, unsupported or password-protected.
        return Err(PdfFailure::Unreadable);
    }
    let result = read_pages(document, max_page_pixels);
    if !threw() {
        unsafe { vf_pdf_close(document) };
    }
    result
}

/// Whether a C++ exception was caught beneath a PDFium call.
fn threw() -> bool {
    unsafe { vf_pdf_failure() != 0 }
}

/// Runs one PDFium call. After a C++ exception PDFium's state is unknown, so
/// the error is returned at once and nothing calls PDFium again.
fn pdfium<T>(call: impl FnOnce() -> T) -> Result<T, PdfFailure> {
    let value = call();
    match unsafe { vf_pdf_failure() } {
        0 => Ok(value),
        OUT_OF_MEMORY => Err(PdfFailure::LimitExceeded),
        _ => Err(PdfFailure::Unreadable),
    }
}

fn read_pages(document: Handle, max_page_pixels: u64) -> Result<PdfPayloads, PdfFailure> {
    let pages = pdfium(|| unsafe { vf_pdf_page_count(document) })?;
    if pages > MAX_PAGES {
        return Err(PdfFailure::TooManyPages);
    }

    let mut codes: Vec<String> = Vec::new();
    let mut code_bytes = 0;
    let mut total_pixels: u64 = 0;
    for index in 0..pages.max(0) {
        let page = pdfium(|| unsafe { vf_pdf_load_page(document, index) })?;
        if page.is_null() {
            continue;
        }
        let rendered = render_page(page, max_page_pixels, &mut total_pixels);
        if !threw() {
            unsafe { vf_pdf_close_page(page) };
        }
        for code in rendered? {
            // Stop early rather than hold every page's codes before refusing them.
            code_bytes += code.len();
            if code_bytes > MAX_RESULT_BYTES {
                return Err(PdfFailure::ResultTooLarge);
            }
            codes.push(code);
        }
    }
    Ok(PdfPayloads {
        metadata: Vec::new(),
        codes,
        pages,
    })
}

fn render_page(
    page: Handle,
    max_page_pixels: u64,
    total_pixels: &mut u64,
) -> Result<Vec<String>, PdfFailure> {
    let (width, height) = page_size(page, max_page_pixels)?.ok_or(PdfFailure::Unreadable)?;
    *total_pixels += width as u64 * height as u64;
    if *total_pixels > MAX_TOTAL_PIXELS {
        return Err(PdfFailure::LimitExceeded);
    }

    let bitmap = pdfium(|| unsafe { vf_pdf_render(page, width, height) })?;
    if bitmap.is_null() {
        return Err(PdfFailure::LimitExceeded);
    }
    let gray = to_gray(bitmap, width, height)?;
    pdfium(|| unsafe { vf_pdf_bitmap_destroy(bitmap) })?;
    match qr::read_codes(&gray, width as u32, height as u32) {
        Ok(codes) => Ok(codes),
        Err(qr::QrFailure::OutOfMemory) => Err(PdfFailure::LimitExceeded),
        Err(qr::QrFailure::Failed) => Ok(Vec::new()),
    }
}

/// The page's size in pixels at `RENDER_DPI`, scaled down to fit `max_pixels`;
/// `None` for a page without a usable size.
fn page_size(page: Handle, max_pixels: u64) -> Result<Option<(i32, i32)>, PdfFailure> {
    let points_wide = pdfium(|| unsafe { vf_pdf_page_width(page) })?;
    let points_high = pdfium(|| unsafe { vf_pdf_page_height(page) })?;
    if !(points_wide.is_finite() && points_high.is_finite())
        || points_wide <= 0.0
        || points_high <= 0.0
    {
        return Ok(None);
    }
    let mut scale = RENDER_DPI / 72.0;
    let pixels = (points_wide * scale) as f64 * (points_high * scale) as f64;
    if pixels > max_pixels as f64 {
        scale *= (max_pixels as f64 / pixels).sqrt() as f32;
    }
    let width = (points_wide * scale) as i32;
    let height = (points_high * scale) as i32;
    Ok((width > 0 && height > 0).then_some((width, height)))
}

/// PDFium's BGRx rows to tightly packed greyscale, with ZXing's weighting.
fn to_gray(bitmap: Handle, width: i32, height: i32) -> Result<Vec<u8>, PdfFailure> {
    let rows = pdfium(|| unsafe { vf_pdf_bitmap_rows(bitmap) })?;
    let stride = pdfium(|| unsafe { vf_pdf_bitmap_stride(bitmap) })? as usize;
    let mut gray = Vec::with_capacity(width as usize * height as usize);
    for row in 0..height as usize {
        let pixels =
            unsafe { std::slice::from_raw_parts(rows.add(row * stride), width as usize * 4) };
        gray.extend(pixels.as_chunks::<4>().0.iter().map(|bgrx| {
            ((117 * bgrx[0] as u32 + 601 * bgrx[1] as u32 + 306 * bgrx[2] as u32 + 0x200) >> 10)
                as u8
        }));
    }
    Ok(gray)
}
