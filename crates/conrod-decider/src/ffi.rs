//! The slice of llama.cpp (`llama`, `mtmd`, `ggml` DLLs, release b12359) this crate
//! drives, bound at run time.
//!
//! The big parameter structs are never declared in full: the library fills them with its
//! own defaults and only fields at stable early offsets are overwritten (see the
//! `*_OFFSET` constants, taken from `llama.h` / `mtmd.h` at the pinned tag). The blobs
//! are over-sized so a newer library can never write past them.

use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

pub type LlamaToken = i32;
pub type LlamaPos = i32;
pub type Dev = *mut c_void;

/// An over-sized, 8-aligned buffer standing in for a by-value C parameter struct.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct Blob(pub [u8; 1024]);

impl Blob {
    pub fn set<T: Copy>(&mut self, offset: usize, value: T) {
        assert!(offset + std::mem::size_of::<T>() <= self.0.len());
        unsafe { std::ptr::write_unaligned(self.0.as_mut_ptr().add(offset) as *mut T, value) }
    }
}

// llama_model_params
pub const MODEL_DEVICES_OFFSET: usize = 0;
pub const MODEL_N_GPU_LAYERS_OFFSET: usize = 16;
// llama_context_params
pub const CTX_N_CTX: usize = 0;
pub const CTX_N_BATCH: usize = 4;
pub const CTX_N_UBATCH: usize = 8;
pub const CTX_N_SEQ_MAX: usize = 12;
pub const CTX_N_OUTPUTS_MAX: usize = 20;
pub const CTX_N_OUTPUTS_MAX_PER_SEQ: usize = 24;
pub const CTX_N_THREADS: usize = 28;
pub const CTX_N_THREADS_BATCH: usize = 32;
// mtmd_context_params
pub const MTMD_USE_GPU: usize = 0;
pub const MTMD_DEVICE: usize = 8;
pub const MTMD_N_THREADS: usize = 20;
// llama_model_quantize_params
pub const QUANT_NTHREAD: usize = 0;
pub const QUANT_FTYPE: usize = 4;
pub const QUANT_ALLOW_REQUANTIZE: usize = 16;
pub const FTYPE_Q4_K_M: i32 = 15;

#[repr(C)]
pub struct LlamaEmbd {
    pub data: *const f32,
    pub n_rows: usize,
    pub n_embd: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MtmdDecoderPos {
    pub t: u32,
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

#[repr(C)]
pub struct MtmdInputText {
    pub text: *const c_char,
    pub text_len: usize,
    pub add_special: bool,
    pub parse_special: bool,
}

pub const CHUNK_TEXT: c_int = 0;
pub const CHUNK_IMAGE: c_int = 1;
pub const PROCESS_DECODE: c_int = 1;
pub const DEVICE_GPU: c_int = 1;

pub type LogCallback = unsafe extern "C" fn(level: c_int, text: *const c_char, user: *mut c_void);

macro_rules! libs_of {
    (llama, $l:ident, $m:ident, $g:ident, $b:ident) => {
        [&$l]
    };
    (mtmd, $l:ident, $m:ident, $g:ident, $b:ident) => {
        [&$m]
    };
    (ggml, $l:ident, $m:ident, $g:ident, $b:ident) => {
        [&$g, &$b]
    };
}

/// Find `name` in the first of `libs` that exports it.
unsafe fn sym<T: Copy>(libs: &[&Library], name: &str) -> Result<T, String> {
    let mut last = String::new();
    for lib in libs {
        match lib.get::<T>(format!("{name}\0").as_bytes()) {
            Ok(s) => return Ok(*s),
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("{name}: {last}"))
}

// Binds every listed symbol from the DLL it lives in.
macro_rules! bind_all {
    ($( $lib:ident . $name:ident : fn($($arg:ty),*) $(-> $ret:ty)? ;)*) => {
        #[allow(non_snake_case)]
        pub struct Api {
            $( pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )*
            _libs: Vec<Library>,
        }
        impl Api {
            fn bind(llama: Library, mtmd: Library, ggml: Library, base: Library) -> Result<Api, String> {
                unsafe {
                    $( let $name = sym::<unsafe extern "C" fn($($arg),*) $(-> $ret)?>(
                        &libs_of!($lib, llama, mtmd, ggml, base), stringify!($name))?; )*
                    Ok(Api { $( $name, )* _libs: vec![llama, mtmd, ggml, base] })
                }
            }
        }
    };
}
bind_all! {
    ggml.ggml_backend_load_all_from_path: fn(*const c_char);
    ggml.ggml_backend_dev_count: fn() -> usize;
    ggml.ggml_backend_dev_get: fn(usize) -> Dev;
    ggml.ggml_backend_dev_name: fn(Dev) -> *const c_char;
    ggml.ggml_backend_dev_description: fn(Dev) -> *const c_char;
    ggml.ggml_backend_dev_memory: fn(Dev, *mut usize, *mut usize);
    ggml.ggml_backend_dev_type: fn(Dev) -> c_int;
    llama.llama_log_set: fn(Option<LogCallback>, *mut c_void);
    llama.llama_backend_init: fn();
    llama.llama_model_default_params: fn() -> Blob;
    llama.llama_context_default_params: fn() -> Blob;
    llama.llama_model_load_from_file: fn(*const c_char, Blob) -> *mut c_void;
    llama.llama_model_free: fn(*mut c_void);
    llama.llama_init_from_model: fn(*mut c_void, Blob) -> *mut c_void;
    llama.llama_free: fn(*mut c_void);
    llama.llama_model_get_vocab: fn(*const c_void) -> *const c_void;
    llama.llama_model_n_embd_inp: fn(*const c_void) -> i32;
    llama.llama_model_size: fn(*const c_void) -> u64;
    llama.llama_model_meta_count: fn(*const c_void) -> i32;
    llama.llama_model_meta_key_by_index: fn(*const c_void, i32, *mut c_char, usize) -> i32;
    llama.llama_model_meta_val_str_by_index: fn(*const c_void, i32, *mut c_char, usize) -> i32;
    llama.llama_vocab_n_tokens: fn(*const c_void) -> i32;
    llama.llama_tokenize: fn(*const c_void, *const c_char, i32, *mut LlamaToken, i32, bool, bool) -> i32;
    llama.llama_get_memory: fn(*const c_void) -> *mut c_void;
    llama.llama_memory_clear: fn(*mut c_void, bool);
    llama.llama_batch_ext_init: fn(*mut c_void) -> *mut c_void;
    llama.llama_batch_ext_free: fn(*mut c_void);
    llama.llama_batch_ext_clear: fn(*mut c_void);
    llama.llama_batch_ext_add_token: fn(*mut c_void, i32, LlamaToken) -> i32;
    llama.llama_batch_ext_add_embd: fn(*mut c_void, i32, LlamaEmbd) -> i32;
    llama.llama_batch_ext_set_pos: fn(*mut c_void, i32, *const LlamaPos) -> bool;
    llama.llama_batch_ext_set_output_logits: fn(*mut c_void, i32, bool) -> bool;
    llama.llama_process: fn(*mut c_void, c_int, *mut c_void) -> i32;
    llama.llama_synchronize: fn(*mut c_void);
    llama.llama_get_logits_ith: fn(*mut c_void, i32) -> *mut f32;
    llama.llama_model_quantize_default_params: fn() -> Blob;
    llama.llama_model_quantize: fn(*const c_char, *const c_char, *const Blob) -> u32;
    mtmd.mtmd_log_set: fn(Option<LogCallback>, *mut c_void);
    mtmd.mtmd_context_params_default: fn() -> Blob;
    mtmd.mtmd_init_from_file: fn(*const c_char, *const c_void, Blob) -> *mut c_void;
    mtmd.mtmd_free: fn(*mut c_void);
    mtmd.mtmd_default_marker: fn() -> *const c_char;
    mtmd.mtmd_decode_use_mrope: fn(*const c_void) -> bool;
    mtmd.mtmd_bitmap_init: fn(u32, u32, *const u8) -> *mut c_void;
    mtmd.mtmd_bitmap_free: fn(*mut c_void);
    mtmd.mtmd_input_chunks_init: fn() -> *mut c_void;
    mtmd.mtmd_input_chunks_size: fn(*const c_void) -> usize;
    mtmd.mtmd_input_chunks_get: fn(*const c_void, usize) -> *const c_void;
    mtmd.mtmd_input_chunks_free: fn(*mut c_void);
    mtmd.mtmd_input_chunk_get_type: fn(*const c_void) -> c_int;
    mtmd.mtmd_input_chunk_get_tokens_text: fn(*const c_void, *mut usize) -> *const LlamaToken;
    mtmd.mtmd_input_chunk_get_tokens_image: fn(*const c_void) -> *const c_void;
    mtmd.mtmd_input_chunk_get_n_tokens: fn(*const c_void) -> usize;
    mtmd.mtmd_input_chunk_get_n_pos: fn(*const c_void) -> LlamaPos;
    mtmd.mtmd_tokenize: fn(*const c_void, *mut c_void, *const MtmdInputText, *const *const c_void, usize) -> i32;
    mtmd.mtmd_encode_chunk: fn(*mut c_void, *const c_void) -> i32;
    mtmd.mtmd_get_output_embd: fn(*mut c_void) -> *mut f32;
    mtmd.mtmd_image_tokens_get_decoder_pos: fn(*const c_void, LlamaPos, usize) -> MtmdDecoderPos;
}

/// The DLLs every install needs, in dependency order.
pub const RUNTIME_DLLS: [&str; 4] = ["ggml-base.dll", "ggml.dll", "llama.dll", "mtmd.dll"];

impl Api {
    /// Load the runtime from `dir` and register its compute backends (Vulkan, CPU).
    pub fn load(dir: &Path) -> Result<Api, String> {
        let open = |name: &str| -> Result<Library, String> {
            let path = dir.join(name);
            #[cfg(windows)]
            let lib = unsafe {
                // LOAD_WITH_ALTERED_SEARCH_PATH: sibling DLLs resolve from `dir`.
                libloading::os::windows::Library::load_with_flags(&path, 0x0000_0008)
                    .map(Library::from)
            };
            #[cfg(not(windows))]
            let lib = unsafe { Library::new(&path) };
            lib.map_err(|e| format!("{}: {e}", path.display()))
        };
        for dll in RUNTIME_DLLS {
            if !dir.join(dll).is_file() {
                return Err(format!("{} is missing from {}", dll, dir.display()));
            }
        }
        let base = open("ggml-base.dll")?;
        let ggml = open("ggml.dll")?;
        let llama = open("llama.dll")?;
        let mtmd = open("mtmd.dll")?;
        let api = Api::bind(llama, mtmd, ggml, base)?;
        let c = CString::new(dir.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        unsafe {
            (api.llama_log_set)(Some(log_sink), std::ptr::null_mut());
            (api.mtmd_log_set)(Some(log_sink), std::ptr::null_mut());
            (api.llama_backend_init)();
            (api.ggml_backend_load_all_from_path)(c.as_ptr());
        }
        Ok(api)
    }

    pub fn string(&self, p: *const c_char) -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }
}

/// llama.cpp's log, kept quiet unless `CONROD_DECIDER_LOG` is set.
unsafe extern "C" fn log_sink(_level: c_int, text: *const c_char, _user: *mut c_void) {
    if text.is_null() || std::env::var_os("CONROD_DECIDER_LOG").is_none() {
        return;
    }
    eprint!("{}", CStr::from_ptr(text).to_string_lossy());
}
