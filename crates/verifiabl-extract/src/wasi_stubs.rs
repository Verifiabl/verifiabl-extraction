//! Definitions for the WASI calls wasi-libc makes, so wasm-ld resolves them
//! here instead of emitting imports. Nothing reaches the host: there is no
//! environment, no clock, no randomness and no file to read or write.

const SUCCESS: u16 = 0;
const EBADF: u16 = 8;
const ENOSYS: u16 = 52;

#[no_mangle]
pub unsafe extern "C" fn __imported_wasi_snapshot_preview1_environ_sizes_get(
    count: *mut u32,
    size: *mut u32,
) -> u16 {
    *count = 0;
    *size = 0;
    SUCCESS
}

#[no_mangle]
pub extern "C" fn __imported_wasi_snapshot_preview1_environ_get(_: usize, _: usize) -> u16 {
    SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn __imported_wasi_snapshot_preview1_clock_time_get(
    _: u32,
    _: u64,
    time: *mut u64,
) -> u16 {
    *time = 0;
    SUCCESS
}

#[no_mangle]
pub extern "C" fn __imported_wasi_snapshot_preview1_random_get(_: usize, _: usize) -> u16 {
    ENOSYS
}

#[no_mangle]
pub extern "C" fn __imported_wasi_snapshot_preview1_proc_exit(_: u32) -> ! {
    core::arch::wasm32::unreachable()
}

macro_rules! bad_descriptor {
    ($($name:ident($($argument:ty),*)),* $(,)?) => {
        $(
            #[no_mangle]
            pub extern "C" fn $name($(_: $argument),*) -> u16 {
                EBADF
            }
        )*
    };
}

bad_descriptor! {
    __imported_wasi_snapshot_preview1_fd_close(u32),
    __imported_wasi_snapshot_preview1_fd_fdstat_get(u32, usize),
    __imported_wasi_snapshot_preview1_fd_fdstat_set_flags(u32, u32),
    __imported_wasi_snapshot_preview1_fd_filestat_get(u32, usize),
    __imported_wasi_snapshot_preview1_fd_prestat_get(u32, usize),
    __imported_wasi_snapshot_preview1_fd_prestat_dir_name(u32, usize, usize),
    __imported_wasi_snapshot_preview1_fd_read(u32, usize, usize, usize),
    __imported_wasi_snapshot_preview1_fd_seek(u32, i64, u32, usize),
    __imported_wasi_snapshot_preview1_fd_sync(u32),
    __imported_wasi_snapshot_preview1_fd_write(u32, usize, usize, usize),
}

// PDFium looks for system fonts; there are none, so it uses its built-in fonts.
bad_descriptor! {
    __imported_wasi_snapshot_preview1_fd_pread(u32, usize, usize, u64, usize),
    __imported_wasi_snapshot_preview1_fd_pwrite(u32, usize, usize, u64, usize),
    __imported_wasi_snapshot_preview1_fd_readdir(u32, usize, usize, u64, usize),
    __imported_wasi_snapshot_preview1_fd_filestat_set_size(u32, u64),
    __imported_wasi_snapshot_preview1_path_filestat_get(u32, u32, usize, usize, usize),
    __imported_wasi_snapshot_preview1_path_unlink_file(u32, usize, usize),
    __imported_wasi_snapshot_preview1_path_remove_directory(u32, usize, usize),
}

#[no_mangle]
pub extern "C" fn __imported_wasi_snapshot_preview1_path_open(
    _: u32,
    _: u32,
    _: usize,
    _: usize,
    _: u32,
    _: u64,
    _: u64,
    _: u32,
    _: usize,
) -> u16 {
    ENOSYS
}
