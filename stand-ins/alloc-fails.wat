;; vf_alloc traps, and vf_out_of_memory then reports a failed allocation, as
;; the module does when its memory cannot hold the input. Pixels can be
;; hundreds of megabytes, and no fixture is that large. The host must report
;; outOfMemory for a trap in vf_alloc, as it does for a trap in a read.
(module
  (memory (export "memory") 1 8192)
  (func (export "vf_abi_version") (result i32) (i32.const 3))
  (func (export "vf_alloc") (param i32) (result i32) (unreachable))
  (func $read (param i32 i32 i32) (result i32)
    (i32.const 0)
  )
  (export "vf_read_image" (func $read))
  (export "vf_read_pdf" (func $read))
  (func (export "vf_read_pixels") (param i32 i32 i32 i32 i32 i32) (result i32)
    (call $read (local.get 0) (local.get 1) (local.get 5))
  )
  (func (export "vf_out_of_memory") (result i32) (i32.const 1))
  (func (export "vf_result_ptr") (result i32) (i32.const 0))
  (func (export "vf_result_len") (result i32) (i32.const 0))
)
