// Reads QR codes in a greyscale image with ZXing-C++ and passes each code's
// text to vf_zxing_emit, defined in Rust. Exceptions never cross into Rust.
#include <cstddef>
#include <cstdint>
#include <new>

#include "ReadBarcode.h"

extern "C" void vf_zxing_emit(void* context, const char* text, size_t length);

namespace {

constexpr int Ok = 0;
constexpr int Failed = -1;
constexpr int OutOfMemory = -2;

ZXing::ReaderOptions QrOptions() {
    ZXing::ReaderOptions options;
    options.setFormats(ZXing::BarcodeFormat::QRCode);
    options.setTryHarder(true);
    options.setTryRotate(true);
    // Light-on-dark codes too, so a code of either polarity counts as ambiguous.
    options.setTryInvert(true);
    options.setTextMode(ZXing::TextMode::Plain);
    options.setMaxNumberOfSymbols(0xFF);
    return options;
}

}  // namespace

extern "C" int vf_zxing_read(const uint8_t* gray, int width, int height, void* context) {
    try {
        ZXing::ImageView view(gray, width, height, ZXing::ImageFormat::Lum);
        for (const ZXing::Barcode& barcode : ZXing::ReadBarcodes(view, QrOptions())) {
            if (barcode.isValid()) {
                const std::string& text = barcode.text();
                vf_zxing_emit(context, text.data(), text.size());
            }
        }
        return Ok;
    } catch (const std::bad_alloc&) {
        return OutOfMemory;
    } catch (...) {
        return Failed;
    }
}
