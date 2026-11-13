// Calls PDFium for src/pdf.rs and keeps C++ exceptions inside the module.
// PDFium is built without exception support, so one thrown beneath it, such as
// std::bad_alloc from operator new at the memory cap, passes through its
// frames uncaught. Each call here catches it and records it for
// vf_pdf_failure. PDFium's state is then unknown: the caller must not call it again.
#include <cstddef>
#include <cstdint>
#include <new>

#include "fpdfview.h"

namespace {

constexpr int None = 0;
constexpr int Failed = -1;
constexpr int OutOfMemory = -2;

constexpr unsigned long White = 0xFFFFFFFF;

int failure = None;
bool initialized = false;

// Runs one PDFium call; after an exception, returns null or zero.
template <typename Call>
auto Guard(Call call) -> decltype(call()) {
    try {
        return call();
    } catch (const std::bad_alloc&) {
        failure = OutOfMemory;
    } catch (...) {
        failure = Failed;
    }
    return decltype(call())();
}

}  // namespace

extern "C" {

// 0, or why the last failed call threw: -1, or -2 for a failed allocation.
int vf_pdf_failure() {
    return failure;
}

// The document, or null when it is damaged, unsupported or password-protected.
FPDF_DOCUMENT vf_pdf_open(const uint8_t* data, size_t size) {
    return Guard([&]() -> FPDF_DOCUMENT {
        if (!initialized) {
            initialized = true;
            FPDF_InitLibrary();
        }
        return FPDF_LoadMemDocument64(data, size, nullptr);
    });
}

int vf_pdf_page_count(FPDF_DOCUMENT document) {
    return Guard([&] { return FPDF_GetPageCount(document); });
}

FPDF_PAGE vf_pdf_load_page(FPDF_DOCUMENT document, int index) {
    return Guard([&] { return FPDF_LoadPage(document, index); });
}

float vf_pdf_page_width(FPDF_PAGE page) {
    return Guard([&] { return FPDF_GetPageWidthF(page); });
}

float vf_pdf_page_height(FPDF_PAGE page) {
    return Guard([&] { return FPDF_GetPageHeightF(page); });
}

// The page drawn on white as BGRx, with its annotations, where some issuers
// place a badge; or null when the bitmap cannot be allocated.
FPDF_BITMAP vf_pdf_render(FPDF_PAGE page, int width, int height) {
    return Guard([&]() -> FPDF_BITMAP {
        FPDF_BITMAP bitmap = FPDFBitmap_Create(width, height, 0);
        if (bitmap != nullptr) {
            FPDFBitmap_FillRect(bitmap, 0, 0, width, height, White);
            FPDF_RenderPageBitmap(bitmap, page, 0, 0, width, height, 0, FPDF_ANNOT);
        }
        return bitmap;
    });
}

const uint8_t* vf_pdf_bitmap_rows(FPDF_BITMAP bitmap) {
    return Guard([&] { return static_cast<const uint8_t*>(FPDFBitmap_GetBuffer(bitmap)); });
}

int vf_pdf_bitmap_stride(FPDF_BITMAP bitmap) {
    return Guard([&] { return FPDFBitmap_GetStride(bitmap); });
}

void vf_pdf_bitmap_destroy(FPDF_BITMAP bitmap) {
    Guard([&] { FPDFBitmap_Destroy(bitmap); });
}

void vf_pdf_close_page(FPDF_PAGE page) {
    Guard([&] { FPDF_ClosePage(page); });
}

void vf_pdf_close(FPDF_DOCUMENT document) {
    Guard([&] { FPDF_CloseDocument(document); });
}

}  // extern "C"
