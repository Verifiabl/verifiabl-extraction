//! The image decoding around the png, zune-jpeg and image-webp crates: the
//! size checks, the greyscale conversion and the handling of truncated files.
//! Also the checks and the conversion of pixels that a caller decoded. The QR
//! reader trusts the width and height it is given, so they must match the
//! pixels.

use verifiabl_extract::image::{decode_file, gray_pixels, PixelFormat};

/// Small, so each input decodes quickly; the limit is checked the same way at any size.
const MAX_PIXELS: u64 = 1 << 20;
/// Smaller still, so the pixels of a short input can be on each side of it.
const MAX_RAW_PIXELS: u64 = 1 << 10;
/// Bytes of input that give the shape of the pixels after them.
const SHAPE_BYTES: usize = 4;

pub fn check(input: &[u8]) {
    check_file(input);
    check_pixels(input);
}

fn check_file(file: &[u8]) {
    if let Ok(decoded) = decode_file(file, MAX_PIXELS) {
        let pixels = decoded.width as u64 * decoded.height as u64;
        assert!(
            pixels > 0 && pixels <= MAX_PIXELS,
            "an image outside the pixel limit"
        );
        assert_eq!(
            decoded.gray.len() as u64,
            pixels,
            "pixels do not match the size"
        );
    }
}

/// The shape that an input gives its pixels: a format code, a width, and a
/// height near the one that the pixels fill. Byte mutation alone rarely makes
/// a width and height that match a length, and the matching shapes are the
/// ones that reach the conversion.
struct Shape {
    format: u32,
    width: u32,
    height: u32,
}

impl Shape {
    fn from(bytes: &[u8], pixel_bytes: usize) -> Shape {
        let format = bytes[0] as u32;
        let width = u16::from_le_bytes([bytes[1], bytes[2]]) as u32;
        let row_bytes = width.max(1) as usize * bytes_per_pixel(format);
        let filled = (pixel_bytes / row_bytes) as u32;
        let height = match bytes[3] % 4 {
            0 => filled + 1,
            1 => filled.saturating_sub(1),
            _ => filled,
        };
        Shape {
            format,
            width,
            height,
        }
    }

    /// The pixel count, and the bytes that this many pixels take.
    fn sizes(&self) -> (u64, u64) {
        let count = self.width as u64 * self.height as u64;
        (count, count * bytes_per_pixel(self.format) as u64)
    }
}

/// One for a code that is not a format, so that such a code still has a shape.
fn bytes_per_pixel(format: u32) -> usize {
    PixelFormat::from_code(format).map_or(1, PixelFormat::bytes_per_pixel)
}

fn check_pixels(input: &[u8]) {
    let Some((shape, pixels)) = input.split_at_checked(SHAPE_BYTES) else {
        return;
    };
    let shape = Shape::from(shape, pixels.len());
    let (count, bytes) = shape.sizes();
    let format = PixelFormat::from_code(shape.format);
    let readable =
        format.is_some() && (1..=MAX_RAW_PIXELS).contains(&count) && bytes == pixels.len() as u64;

    let mut converted = pixels.to_vec();
    let gray = gray_pixels(
        &mut converted,
        shape.width,
        shape.height,
        shape.format,
        MAX_RAW_PIXELS,
    );
    assert_eq!(
        gray.is_ok(),
        readable,
        "pixels were read or refused against their size"
    );
    let Ok(gray) = gray else {
        return;
    };
    assert_eq!(gray.len() as u64, count, "pixels do not match the size");
    if format == Some(PixelFormat::Gray8) {
        assert_eq!(gray, pixels, "greyscale pixels changed");
        return;
    }
    for (gray, [first, green, third, alpha]) in gray.iter().zip(pixels.as_chunks::<4>().0) {
        if *alpha == 0 {
            assert_eq!(*gray, 0xFF, "a transparent pixel is not white");
        }
        if *alpha == 0xFF && first == green && green == third {
            assert_eq!(gray, green, "an opaque grey pixel changed");
        }
    }
}
