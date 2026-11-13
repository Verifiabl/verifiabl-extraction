;; Reports a PDF read as a success with a page count of -1, which no PDF has.
;; Its image and pixel reads report unreadable, so only the PDF read tests a host.
(module
  (memory (export "memory") 1 8192)
  (data (i32.const 0) "{\"metadata\":[],\"codes\":[],\"pages\":-1}")
  (func (export "vf_abi_version") (result i32) (i32.const 3))
  (func (export "vf_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "vf_read_image") (param i32 i32 i32) (result i32) (i32.const 2))
  (func (export "vf_read_pdf") (param i32 i32 i32) (result i32) (i32.const 0))
  (func (export "vf_read_pixels") (param i32 i32 i32 i32 i32 i32) (result i32) (i32.const 2))
  (func (export "vf_out_of_memory") (result i32) (i32.const 0))
  (func (export "vf_result_ptr") (result i32) (i32.const 0))
  (func (export "vf_result_len") (result i32) (i32.const 37))
)
