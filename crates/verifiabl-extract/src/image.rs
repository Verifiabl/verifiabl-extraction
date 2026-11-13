//! Gives the QR reader greyscale pixels: it decodes a PNG, JPEG or WebP file,
//! or checks and converts pixels that the caller decoded.

use std::io::Cursor;

const WHITE: u8 = 0xFF;

/// A decoded image: one greyscale byte per pixel, row by row.
#[derive(Debug)]
pub struct Decoded {
    pub gray: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// False when the file ended early: the missing rows are white.
    pub complete: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Unreadable,
    TooManyPixels,
}

/// How a caller stores each pixel: `pixelFormats` in `abi/extraction-abi.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// One greyscale byte.
    Gray8,
    /// Red, green, blue, alpha.
    Rgba32,
    /// Blue, green, red, alpha.
    Bgra32,
}

impl PixelFormat {
    /// The format with this code, or `None` for a code the interface does not define.
    pub fn from_code(code: u32) -> Option<PixelFormat> {
        match code {
            0 => Some(PixelFormat::Gray8),
            1 => Some(PixelFormat::Rgba32),
            2 => Some(PixelFormat::Bgra32),
            _ => None,
        }
    }

    pub fn bytes_per_pixel(self) -> usize {
        self.layout().bytes()
    }

    fn layout(self) -> Layout {
        match self {
            PixelFormat::Gray8 => Layout::Gray,
            PixelFormat::Rgba32 => Layout::Rgba,
            PixelFormat::Bgra32 => Layout::Bgra,
        }
    }
}

/// Checks pixels that the caller decoded, then converts them to greyscale in
/// place. The result is the start of `pixels`: one byte for each pixel.
///
/// `pixels` must hold `width` × `height` pixels of `format` and nothing else.
/// The QR reader trusts the width and height, so a buffer of any other length
/// is refused. No copy is made: the largest buffer is 200 MB of the instance's
/// 512 MiB.
pub fn gray_pixels(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    format: u32,
    max_pixels: u64,
) -> Result<&[u8], Failure> {
    check_size(width as u64, height as u64, max_pixels)?;
    let format = PixelFormat::from_code(format).ok_or(Failure::Unreadable)?;
    // Two 32-bit sizes cannot overflow a 64-bit pixel count, but the bytes of
    // that many pixels can.
    let expected = (width as u64 * height as u64).checked_mul(format.bytes_per_pixel() as u64);
    if expected != Some(pixels.len() as u64) {
        return Err(Failure::Unreadable);
    }
    let count = gray_in_place(pixels, format.layout());
    Ok(&pixels[..count])
}

/// Decodes `file` by its signature, refusing one with more than `max_pixels` pixels.
pub fn decode_file(file: &[u8], max_pixels: u64) -> Result<Decoded, Failure> {
    if file.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(file, max_pixels)
    } else if file.starts_with(b"\xFF\xD8\xFF") {
        decode_jpeg(file, max_pixels)
    } else if file.len() >= 12 && &file[0..4] == b"RIFF" && &file[8..12] == b"WEBP" {
        decode_webp(file, max_pixels)
    } else {
        Err(Failure::Unreadable)
    }
}

/// Checks an image's size before its pixels are decoded or converted.
fn check_size(width: u64, height: u64, max_pixels: u64) -> Result<(), Failure> {
    if width == 0 || height == 0 {
        return Err(Failure::Unreadable);
    }
    if width * height > max_pixels {
        return Err(Failure::TooManyPixels);
    }
    Ok(())
}

fn decode_png(file: &[u8], max_pixels: u64) -> Result<Decoded, Failure> {
    // png's default 64 MiB allocation limit would refuse large valid scans;
    // the instance's memory cap is the bound instead.
    let limits = png::Limits { bytes: usize::MAX };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(file), limits);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| Failure::Unreadable)?;
    let (width, height) = (reader.info().width, reader.info().height);
    check_size(width as u64, height as u64, max_pixels)?;

    let (color, _) = reader.output_color_type();
    let channels = color.samples();
    let size = reader.output_buffer_size().ok_or(Failure::TooManyPixels)?;
    let mut buffer = white(size);
    let complete = match reader.next_frame(&mut buffer) {
        Ok(_) => true,
        Err(png::DecodingError::IoError(error)) if is_truncated(&error) => false,
        Err(_) => return Err(Failure::Unreadable),
    };
    let gray = to_gray(buffer, channels);
    Ok(Decoded {
        gray,
        width,
        height,
        complete,
    })
}

fn decode_jpeg(file: &[u8], max_pixels: u64) -> Result<Decoded, Failure> {
    let options = zune_core::options::DecoderOptions::default()
        .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::Luma);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(Cursor::new(file), options);
    decoder.decode_headers().map_err(|_| Failure::Unreadable)?;
    let (width, height) = decoder.dimensions().ok_or(Failure::Unreadable)?;
    check_size(width as u64, height as u64, max_pixels)?;

    let size = decoder.output_buffer_size().ok_or(Failure::Unreadable)?;
    if size != width * height {
        // Luma output has one byte per pixel; anything else is a decoder surprise.
        return Err(Failure::Unreadable);
    }
    let mut gray = white(size);
    let complete = match decoder.decode_into(&mut gray) {
        Ok(()) => true,
        Err(zune_jpeg::errors::DecodeErrors::ExhaustedData) => false,
        Err(_) => return Err(Failure::Unreadable),
    };
    Ok(Decoded {
        gray,
        width: width as u32,
        height: height as u32,
        complete,
    })
}

fn decode_webp(file: &[u8], max_pixels: u64) -> Result<Decoded, Failure> {
    let mut decoder =
        image_webp::WebPDecoder::new(Cursor::new(file)).map_err(|_| Failure::Unreadable)?;
    let (width, height) = decoder.dimensions();
    check_size(width as u64, height as u64, max_pixels)?;

    let channels = if decoder.has_alpha() { 4 } else { 3 };
    let size = decoder.output_buffer_size().ok_or(Failure::TooManyPixels)?;
    let mut buffer = white(size);
    let complete = match decoder.read_image(&mut buffer) {
        Ok(()) => true,
        Err(image_webp::DecodingError::IoError(error)) if is_truncated(&error) => false,
        Err(_) => return Err(Failure::Unreadable),
    };
    let gray = to_gray(buffer, channels);
    Ok(Decoded {
        gray,
        width,
        height,
        complete,
    })
}

/// A white buffer, filled in chunks. One large fill is a single bulk-memory
/// instruction, which the host cannot interrupt at its time budget.
fn white(size: usize) -> Vec<u8> {
    const CHUNK: usize = 1 << 20;
    let mut buffer = Vec::with_capacity(size);
    while buffer.len() < size {
        let next = (size - buffer.len()).min(CHUNK);
        buffer.resize(buffer.len() + next, WHITE);
    }
    buffer
}

/// A truncated file keeps the rows it has, which may hold the code; other
/// errors mean the file is damaged.
fn is_truncated(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::UnexpectedEof
}

/// The order of the bytes in each pixel of a buffer.
#[derive(Clone, Copy)]
enum Layout {
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
    Bgra,
}

impl Layout {
    /// The layout of a decoder's 8-bit output with this many channels.
    fn of_channels(channels: usize) -> Layout {
        match channels {
            1 => Layout::Gray,
            2 => Layout::GrayAlpha,
            3 => Layout::Rgb,
            _ => Layout::Rgba,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Layout::Gray => 1,
            Layout::GrayAlpha => 2,
            Layout::Rgb => 3,
            Layout::Rgba | Layout::Bgra => 4,
        }
    }

    /// A pixel's brightness and opacity, each from 0 to 255.
    fn luma_and_alpha(self, pixel: &[u8]) -> (u32, u32) {
        match self {
            Layout::Gray => (pixel[0] as u32, 255),
            Layout::GrayAlpha => (pixel[0] as u32, pixel[1] as u32),
            Layout::Rgb => (luminance(pixel[0], pixel[1], pixel[2]), 255),
            Layout::Rgba => (luminance(pixel[0], pixel[1], pixel[2]), pixel[3] as u32),
            Layout::Bgra => (luminance(pixel[2], pixel[1], pixel[0]), pixel[3] as u32),
        }
    }
}

/// Converts a decoder's 8-bit pixels to greyscale in place.
fn to_gray(mut buffer: Vec<u8>, channels: usize) -> Vec<u8> {
    let count = gray_in_place(&mut buffer, Layout::of_channels(channels));
    // Not shrunk: a reallocation would briefly hold both buffers.
    buffer.truncate(count);
    buffer
}

/// Writes each pixel's greyscale byte at the start of `buffer` and returns
/// how many pixels it holds. Any alpha is composited onto white, so a
/// transparent badge background reads as paper.
fn gray_in_place(buffer: &mut [u8], layout: Layout) -> usize {
    let bytes = layout.bytes();
    let count = buffer.len() / bytes;
    if let Layout::Gray = layout {
        return count;
    }
    for index in 0..count {
        let start = index * bytes;
        let (luma, alpha) = layout.luma_and_alpha(&buffer[start..start + bytes]);
        let composited = (luma * alpha + 255 * (255 - alpha) + 127) / 255;
        buffer[index] = composited as u8;
    }
    count
}

/// ZXing's weighting, so greyscale matches what its RGB source would compute.
fn luminance(red: u8, green: u8, blue: u8) -> u32 {
    (306 * red as u32 + 601 * green as u32 + 117 * blue as u32 + 0x200) >> 10
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = include_bytes!("../../../fixtures/qr.png");
    const JPEG: &[u8] = include_bytes!("../../../fixtures/qr-photo.jpg");
    const WEBP: &[u8] = include_bytes!("../../../fixtures/blank.webp");
    const MAX_PIXELS: u64 = 50_000_000;

    #[test]
    fn decodes_each_format_to_one_byte_per_pixel() {
        for file in [PNG, JPEG, WEBP] {
            let decoded = decode_file(file, MAX_PIXELS).expect("the fixture decodes");
            assert!(decoded.complete);
            assert_eq!(
                decoded.gray.len(),
                decoded.width as usize * decoded.height as usize
            );
        }
    }

    #[test]
    fn refuses_an_image_over_the_pixel_limit_from_its_header() {
        assert_eq!(decode_file(PNG, 1).unwrap_err(), Failure::TooManyPixels);
        assert_eq!(decode_file(JPEG, 1).unwrap_err(), Failure::TooManyPixels);
    }

    #[test]
    fn keeps_the_rows_of_a_truncated_file() {
        let png = decode_file(&PNG[..PNG.len() / 2], MAX_PIXELS).expect("the header is whole");
        assert!(!png.complete);
        assert_eq!(png.gray.len(), png.width as usize * png.height as usize);
        assert_eq!(png.gray.last(), Some(&WHITE));

        // zune-jpeg fills a truncated scan itself and reports the image as whole.
        let jpeg = decode_file(&JPEG[..JPEG.len() / 2], MAX_PIXELS).expect("the header is whole");
        assert_eq!(jpeg.gray.len(), jpeg.width as usize * jpeg.height as usize);
    }

    #[test]
    fn refuses_other_files() {
        assert_eq!(
            decode_file(b"", MAX_PIXELS).unwrap_err(),
            Failure::Unreadable
        );
        assert_eq!(
            decode_file(b"%PDF-1.7\n", MAX_PIXELS).unwrap_err(),
            Failure::Unreadable
        );
        assert_eq!(
            decode_file(&PNG[..20], MAX_PIXELS).unwrap_err(),
            Failure::Unreadable
        );
    }

    #[test]
    fn composites_transparency_onto_white() {
        // Transparent black and opaque black, as RGBA.
        let gray = to_gray(vec![0, 0, 0, 0, 0, 0, 0, 255], 4);
        assert_eq!(gray, [255, 0]);
    }

    const GRAY8: u32 = 0;
    const RGBA32: u32 = 1;
    const BGRA32: u32 = 2;

    fn gray(pixels: &[u8], width: u32, height: u32, format: u32) -> Result<Vec<u8>, Failure> {
        let mut pixels = pixels.to_vec();
        gray_pixels(&mut pixels, width, height, format, MAX_PIXELS).map(<[u8]>::to_vec)
    }

    #[test]
    fn converts_pixels_of_each_format() {
        // Opaque red, then transparent black.
        let red_first = [255, 0, 0, 255, 0, 0, 0, 0];
        assert_eq!(gray(&[7, 200], 2, 1, GRAY8).unwrap(), [7, 200]);
        assert_eq!(gray(&red_first, 2, 1, RGBA32).unwrap(), [76, 255]);
        // The same bytes are opaque blue when blue is first.
        assert_eq!(gray(&red_first, 2, 1, BGRA32).unwrap(), [29, 255]);
    }

    #[test]
    fn refuses_pixels_whose_length_does_not_match_their_size() {
        let pixels = [0; 24];
        assert_eq!(gray(&pixels, 2, 3, RGBA32).unwrap().len(), 6);
        for (width, height, format) in [
            (2, 3, GRAY8),
            (3, 3, RGBA32),
            (2, 2, BGRA32),
            (0, 3, RGBA32),
            (2, 0, RGBA32),
        ] {
            assert_eq!(
                gray(&pixels, width, height, format).unwrap_err(),
                Failure::Unreadable,
                "{width}x{height}, format {format}"
            );
        }
    }

    #[test]
    fn refuses_pixels_in_a_format_the_interface_does_not_define() {
        assert_eq!(gray(&[0; 4], 2, 2, 3).unwrap_err(), Failure::Unreadable);
        assert_eq!(
            gray(&[0; 4], 2, 2, u32::MAX).unwrap_err(),
            Failure::Unreadable
        );
    }

    #[test]
    fn refuses_pixels_over_the_pixel_limit_whatever_their_length() {
        let mut pixels = [0; 4];
        assert_eq!(
            gray_pixels(&mut pixels, 2, 2, GRAY8, 3).unwrap_err(),
            Failure::TooManyPixels
        );
        assert_eq!(
            gray_pixels(&mut pixels, 5000, 10_001, RGBA32, MAX_PIXELS).unwrap_err(),
            Failure::TooManyPixels
        );
    }

    #[test]
    fn refuses_a_size_whose_bytes_overflow() {
        // 4 bytes for each of 2^64 - 2^33 + 1 pixels do not fit in 64 bits.
        assert_eq!(
            gray_pixels(&mut [], u32::MAX, u32::MAX, RGBA32, u64::MAX).unwrap_err(),
            Failure::Unreadable
        );
    }
}
