//! Perplexity's decision model (`pplx-decider-flash`) run in-process.
//!
//! The model answers many questions about one state (text and/or images) in a single
//! forward pass: no generation, a small readout head read at one marker per question.
//! The runtime is a pinned llama.cpp build (`llama`, `mtmd`, `ggml` DLLs) loaded at run
//! time; the weights are the official GGUF conversion. Everything the reference does
//! (prompt, noncausal full attention, per-marker readout, calibrated temperature, image
//! resize) is reproduced here, so no Python and no server is involved.
//!
//! ```no_run
//! use conrod_decider::{Decider, DeciderPaths, Device, LoadOptions, Question, Request};
//! let paths = DeciderPaths::new("runtime", "decider-q8_0.gguf", "mmproj-f16.gguf");
//! let decider = Decider::load(&paths, &LoadOptions::default()).unwrap();
//! let response = decider
//!     .decide(&Request::new("a photo of a car").ask("sharp", Question::noul("Is it sharp?")))
//!     .unwrap();
//! println!("{:?}", response.noul("sharp"));
//! ```

mod ffi;
pub mod gating;
pub mod protocol;
pub mod types;
pub mod vram;

pub use protocol::RequestError;
pub use types::{Answer, NoulCriteria, Ordered, Question, Request, Response, Usage};

use ffi::*;
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Calibrated temperature of `pplx-decider-flash-v1.2` (`decision_config.json`); used
/// only when the GGUF carries none.
pub const DEFAULT_TEMPERATURE: f64 = 1.5539092221462472;
/// Images are resized to at most this many pixels (the checkpoint's `max_pixels`).
pub const MAX_PIXELS: u64 = 1_638_400;
/// What Conrod uses by default: 768 x 768 worth of pixels. The model accepts up to
/// [`MAX_PIXELS`]; at that size a frame costs ~1 s and ~1.5 GiB more video memory.
pub const DEFAULT_PIXELS: u64 = 589_824;
/// Most questions one request may carry.
pub const MAX_QUESTIONS: usize = 64;
/// Smaller images keep their size, down to this many pixels.
pub const MIN_PIXELS: u64 = 65_536;

#[derive(Debug)]
pub enum DeciderError {
    /// The runtime or weights could not be loaded.
    Load(String),
    /// The request does not follow the schema.
    Request(RequestError),
    /// The request needs more tokens than this context holds.
    TooLong { tokens: usize, limit: usize },
    /// The forward pass or image encoding failed.
    Run(String),
}

impl std::fmt::Display for DeciderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeciderError::Load(m) => write!(f, "decision model: {m}"),
            DeciderError::Request(e) => write!(f, "decision request: {e}"),
            DeciderError::TooLong { tokens, limit } => {
                write!(f, "decision request needs {tokens} tokens, limit {limit}")
            }
            DeciderError::Run(m) => write!(f, "decision model run: {m}"),
        }
    }
}
impl std::error::Error for DeciderError {}
impl From<RequestError> for DeciderError {
    fn from(e: RequestError) -> Self {
        DeciderError::Request(e)
    }
}
pub type Result<T> = std::result::Result<T, DeciderError>;

/// Where the pieces live on disk.
#[derive(Debug, Clone)]
pub struct DeciderPaths {
    /// Folder with `llama.dll`, `mtmd.dll`, `ggml*.dll` and the backends.
    pub runtime: PathBuf,
    pub model: PathBuf,
    pub mmproj: PathBuf,
}

impl DeciderPaths {
    pub fn new(
        runtime: impl Into<PathBuf>,
        model: impl Into<PathBuf>,
        mmproj: impl Into<PathBuf>,
    ) -> Self {
        DeciderPaths {
            runtime: runtime.into(),
            model: model.into(),
            mmproj: mmproj.into(),
        }
    }
}

/// Which compute device runs the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    /// The GPU with the most free memory; the CPU when there is none.
    #[default]
    Auto,
    /// The n-th GPU the runtime lists (see [`Decider::devices`]).
    Gpu(usize),
    Cpu,
}

#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub device: Device,
    /// Longest request in tokens (template + state + questions + image positions). A
    /// full-size 1.6 MP image alone is about 1,600.
    pub max_tokens: u32,
    /// Image pixel cap, [`MIN_PIXELS`]..=[`MAX_PIXELS`]. Fewer pixels is faster.
    pub max_pixels: u64,
    pub threads: i32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions {
            device: Device::Auto,
            max_tokens: 4096,
            max_pixels: DEFAULT_PIXELS,
            threads: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub gpu: bool,
    pub free: u64,
    pub total: u64,
}

/// What a request cost, for the live pipeline view and benchmarks.
#[derive(Debug, Clone, Copy, Default)]
pub struct Timing {
    pub tokens: usize,
    pub encode: Duration,
    pub decode: Duration,
}

struct Inner {
    api: Api,
    model: *mut c_void,
    ctx: *mut c_void,
    mtmd: *mut c_void,
    batch: *mut c_void,
    vocab: *const c_void,
    n_vocab: usize,
    n_embd: usize,
    mrope: bool,
    marker: LlamaToken,
    codes: Vec<String>,
    code_tokens: Vec<LlamaToken>,
    image_marker: String,
    temperature: f64,
    max_tokens: usize,
    max_pixels: u64,
    _devices: Box<[Dev; 2]>,
}

/// A loaded decision model. Requests are serialized; share it behind an `Arc`.
pub struct Decider {
    inner: Mutex<Inner>,
    pub device: Option<DeviceInfo>,
    pub model_bytes: u64,
    pub temperature: f64,
}

unsafe impl Send for Inner {}

impl Decider {
    /// Load the runtime, the weights and the vision projector onto a device.
    pub fn load(paths: &DeciderPaths, options: &LoadOptions) -> Result<Decider> {
        for (what, p) in [("model", &paths.model), ("projector", &paths.mmproj)] {
            if !p.is_file() {
                return Err(DeciderError::Load(format!(
                    "{what} file missing: {}",
                    p.display()
                )));
            }
        }
        let api = Api::load(&paths.runtime).map_err(DeciderError::Load)?;
        let devices = list_devices(&api);
        let chosen = match options.device {
            Device::Cpu => None,
            Device::Gpu(i) => Some(
                devices
                    .iter()
                    .find(|d| d.index == i && d.gpu)
                    .cloned()
                    .ok_or_else(|| DeciderError::Load(format!("no GPU with index {i}")))?,
            ),
            Device::Auto => devices
                .iter()
                .filter(|d| d.gpu)
                .max_by_key(|d| d.free)
                .cloned(),
        };
        let dev_ptr = |d: &DeviceInfo| unsafe { (api.ggml_backend_dev_get)(d.index) };
        let mut dev_list: Box<[Dev; 2]> = Box::new([std::ptr::null_mut(); 2]);
        // CPU weights go through the CPU backend, which is always registered.
        let cpu = devices.iter().find(|d| !d.gpu).cloned();
        if let Some(d) = chosen.as_ref().or(cpu.as_ref()) {
            dev_list[0] = dev_ptr(d);
        }
        let path_c = cstr(&paths.model)?;
        let mmproj_c = cstr(&paths.mmproj)?;
        let n = options.max_tokens;
        unsafe {
            let mut mp = (api.llama_model_default_params)();
            mp.set(MODEL_DEVICES_OFFSET, dev_list.as_ptr());
            mp.set(
                MODEL_N_GPU_LAYERS_OFFSET,
                if chosen.is_some() { 999i32 } else { 0i32 },
            );
            let model = (api.llama_model_load_from_file)(path_c.as_ptr(), mp);
            if model.is_null() {
                return Err(DeciderError::Load(
                    "llama.cpp could not load the model (not enough memory, or a corrupt file)"
                        .into(),
                ));
            }
            let mut cp = (api.llama_context_default_params)();
            cp.set(CTX_N_CTX, n);
            cp.set(CTX_N_BATCH, n);
            cp.set(CTX_N_UBATCH, n);
            cp.set(CTX_N_SEQ_MAX, 1u32);
            // One logit row per question: the default (a row per batch token) would be GBs.
            cp.set(CTX_N_OUTPUTS_MAX, MAX_QUESTIONS as u32);
            cp.set(CTX_N_OUTPUTS_MAX_PER_SEQ, MAX_QUESTIONS as u32);
            cp.set(CTX_N_THREADS, options.threads);
            cp.set(CTX_N_THREADS_BATCH, options.threads);
            let ctx = (api.llama_init_from_model)(model, cp);
            if ctx.is_null() {
                (api.llama_model_free)(model);
                return Err(DeciderError::Load(
                    "llama.cpp could not create a context (out of video memory?)".into(),
                ));
            }
            let mut mt = (api.mtmd_context_params_default)();
            mt.set(MTMD_USE_GPU, chosen.is_some());
            mt.set(MTMD_DEVICE, dev_list[0]);
            mt.set(MTMD_N_THREADS, options.threads);
            let mtmd = (api.mtmd_init_from_file)(mmproj_c.as_ptr(), model, mt);
            if mtmd.is_null() {
                (api.llama_free)(ctx);
                (api.llama_model_free)(model);
                return Err(DeciderError::Load(
                    "llama.cpp could not load the vision projector".into(),
                ));
            }
            let vocab = (api.llama_model_get_vocab)(model);
            let n_vocab = (api.llama_vocab_n_tokens)(vocab) as usize;
            let n_embd = (api.llama_model_n_embd_inp)(model) as usize;
            let mut inner = Inner {
                batch: (api.llama_batch_ext_init)(ctx),
                mrope: (api.mtmd_decode_use_mrope)(mtmd),
                image_marker: api.string((api.mtmd_default_marker)()),
                marker: -1,
                codes: Vec::new(),
                code_tokens: Vec::new(),
                temperature: DEFAULT_TEMPERATURE,
                max_tokens: n as usize,
                max_pixels: options.max_pixels.clamp(MIN_PIXELS, MAX_PIXELS),
                _devices: dev_list,
                api,
                model,
                ctx,
                mtmd,
                vocab,
                n_vocab,
                n_embd,
            };
            let model_bytes = (inner.api.llama_model_size)(model);
            inner.read_vocab()?;
            inner.read_temperature();
            let temperature = inner.temperature;
            let decider = Decider {
                inner: Mutex::new(inner),
                device: chosen,
                model_bytes,
                temperature,
            };
            decider.warm_up();
            Ok(decider)
        }
    }

    /// Re-quantize the official Q8_0 GGUF to 4-bit (Q4_K_M) into `output`, on this machine,
    /// so no second multi-GB file has to be hosted. Takes about a minute and a few GB of RAM.
    pub fn quantize_int4(runtime: &Path, input: &Path, output: &Path) -> Result<()> {
        let api = Api::load(runtime).map_err(DeciderError::Load)?;
        let (inp, out) = (cstr(input)?, cstr(output)?);
        unsafe {
            let mut p = (api.llama_model_quantize_default_params)();
            p.set(QUANT_NTHREAD, 0i32);
            p.set(QUANT_FTYPE, FTYPE_Q4_K_M);
            p.set(QUANT_ALLOW_REQUANTIZE, true);
            let rc = (api.llama_model_quantize)(inp.as_ptr(), out.as_ptr(), &p);
            if rc != 0 {
                return Err(DeciderError::Load(format!("quantization failed ({rc})")));
            }
        }
        Ok(())
    }

    /// Devices the runtime found in `dir` (loads the DLLs; for settings and diagnostics).
    pub fn devices(runtime: &Path) -> Result<Vec<DeviceInfo>> {
        let api = Api::load(runtime).map_err(DeciderError::Load)?;
        Ok(list_devices(&api))
    }

    /// One throwaway image request: the GPU backend compiles its kernels on first use
    /// (about 15 s on Vulkan), which must not land on the first real frame.
    fn warm_up(&self) {
        let blank = image::RgbImage::from_pixel(256, 256, image::Rgb([128, 128, 128]));
        let request = Request::new("warm-up")
            .ask("a", Question::noul("Is this an image?"))
            .ask(
                "b",
                Question::choice("Pick one.", [("x", None), ("y", None)]),
            )
            .with_image(blank);
        let _ = self.decide(&request);
        let text_only = Request::new("warm-up").ask("a", Question::noul("Is this text?"));
        let _ = self.decide(&text_only);
    }

    /// Answer one request.
    pub fn decide(&self, request: &Request) -> Result<Response> {
        self.decide_timed(request).map(|(r, _)| r)
    }

    /// Free and total memory of the device the model runs on, bytes.
    pub fn device_memory(&self) -> Option<(u64, u64)> {
        let d = self.device.as_ref()?;
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (mut free, mut total) = (0usize, 0usize);
        unsafe {
            let dev = (inner.api.ggml_backend_dev_get)(d.index);
            (inner.api.ggml_backend_dev_memory)(dev, &mut free, &mut total);
        }
        Some((free as u64, total as u64))
    }

    /// Answer one request and report what it cost.
    pub fn decide_timed(&self, request: &Request) -> Result<(Response, Timing)> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (logits, tokens, timing) = inner.run(request)?;
        let t = inner.temperature;
        let mut answers = Ordered::new();
        for ((id, question), values) in request.questions.iter().zip(&logits) {
            let probabilities = protocol::softmax(values, t);
            answers.push(id.clone(), protocol::answer(question, &probabilities));
        }
        Ok((
            Response {
                answers,
                usage: Usage {
                    input_tokens: tokens as u32,
                    output_tokens: 0,
                    sequences: 1,
                },
            },
            timing,
        ))
    }

    /// Raw readout logits of every question's valid options, before temperature. The
    /// margin `true - false` of a `noul` question ranks without saturating into ties.
    pub fn logits(&self, request: &Request) -> Result<Ordered<Vec<f32>>> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (logits, _, _) = inner.run(request)?;
        Ok(request
            .questions
            .iter()
            .map(|(k, _)| k.clone())
            .zip(logits)
            .collect())
    }
}

impl Drop for Decider {
    fn drop(&mut self) {
        let inner = self.inner.get_mut().unwrap_or_else(|e| e.into_inner());
        unsafe {
            (inner.api.llama_batch_ext_free)(inner.batch);
            (inner.api.mtmd_free)(inner.mtmd);
            (inner.api.llama_free)(inner.ctx);
            (inner.api.llama_model_free)(inner.model);
        }
    }
}

fn cstr(p: &Path) -> Result<CString> {
    CString::new(p.to_string_lossy().as_bytes())
        .map_err(|_| DeciderError::Load(format!("bad path {}", p.display())))
}

fn list_devices(api: &Api) -> Vec<DeviceInfo> {
    unsafe {
        (0..(api.ggml_backend_dev_count)())
            .map(|index| {
                let dev = (api.ggml_backend_dev_get)(index);
                let (mut free, mut total) = (0usize, 0usize);
                (api.ggml_backend_dev_memory)(dev, &mut free, &mut total);
                let kind = (api.ggml_backend_dev_type)(dev);
                DeviceInfo {
                    index,
                    name: api.string((api.ggml_backend_dev_name)(dev)),
                    description: api.string((api.ggml_backend_dev_description)(dev)),
                    gpu: kind == DEVICE_GPU,
                    free: free as u64,
                    total: total as u64,
                }
            })
            .collect()
    }
}

impl Inner {
    fn tokenize_plain(&self, text: &str) -> Vec<LlamaToken> {
        let mut out = vec![0; text.len() + 8];
        let n = unsafe {
            (self.api.llama_tokenize)(
                self.vocab,
                text.as_ptr() as *const c_char,
                text.len() as i32,
                out.as_mut_ptr(),
                out.len() as i32,
                false,
                false,
            )
        };
        out.truncate(n.max(0) as usize);
        out
    }

    /// The marker token and the answer codes `A, B, .., Z, AA, ..` that are one token
    /// each (255 of them, as in `decision_config.json`).
    fn read_vocab(&mut self) -> Result<()> {
        let marker = unsafe {
            let mut t = [0 as LlamaToken; 4];
            let n = (self.api.llama_tokenize)(
                self.vocab,
                protocol::MARKER.as_ptr() as *const c_char,
                protocol::MARKER.len() as i32,
                t.as_mut_ptr(),
                4,
                false,
                true,
            );
            if n != 1 {
                return Err(DeciderError::Load(
                    "the readout marker is not one token".into(),
                ));
            }
            t[0]
        };
        self.marker = marker;
        let letters: Vec<char> = ('A'..='Z').collect();
        let mut candidates: Vec<String> = letters.iter().map(|c| c.to_string()).collect();
        for a in &letters {
            for b in &letters {
                candidates.push(format!("{a}{b}"));
            }
        }
        for code in candidates {
            if self.codes.len() == protocol::MAX_OPTIONS {
                break;
            }
            let t = self.tokenize_plain(&code);
            if t.len() == 1 {
                self.codes.push(code);
                self.code_tokens.push(t[0]);
            }
        }
        if self.codes.len() != protocol::MAX_OPTIONS {
            return Err(DeciderError::Load(
                "answer codes differ from the tokenizer".into(),
            ));
        }
        Ok(())
    }

    /// The temperature the GGUF carries, if it names one.
    fn read_temperature(&mut self) {
        unsafe {
            for i in 0..(self.api.llama_model_meta_count)(self.model) {
                let mut key = vec![0 as c_char; 256];
                (self.api.llama_model_meta_key_by_index)(
                    self.model,
                    i,
                    key.as_mut_ptr(),
                    key.len(),
                );
                let key = self.api.string(key.as_ptr());
                if key.contains("temperature") {
                    let mut val = vec![0 as c_char; 64];
                    (self.api.llama_model_meta_val_str_by_index)(
                        self.model,
                        i,
                        val.as_mut_ptr(),
                        val.len(),
                    );
                    if let Ok(t) = self.api.string(val.as_ptr()).trim().parse::<f64>() {
                        if t.is_finite() && t > 0.0 {
                            self.temperature = t;
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Decoded, resized pixels of every image of the request.
    fn images(&self, request: &Request) -> Result<Vec<image::RgbImage>> {
        let mut out = Vec::new();
        for source in &request.images {
            let img = if let Some(rest) = source.strip_prefix("data:image/") {
                use base64::Engine;
                let b64 = rest
                    .split_once(',')
                    .map(|(_, b)| b)
                    .ok_or_else(|| RequestError("bad data URL".into()))?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .map_err(|e| RequestError(format!("bad data URL: {e}")))?;
                image::load_from_memory(&bytes)
                    .map_err(|e| RequestError(format!("bad image: {e}")))?
            } else {
                image::open(source).map_err(|e| RequestError(format!("{source}: {e}")))?
            };
            out.push(img.to_rgb8());
        }
        out.extend(request.pixels.iter().cloned());
        Ok(out
            .into_iter()
            .map(|img| {
                let (h, w) =
                    protocol::smart_resize(img.height(), img.width(), MIN_PIXELS, self.max_pixels);
                if (h, w) == (img.height(), img.width()) {
                    img
                } else {
                    image::imageops::resize(&img, w, h, image::imageops::FilterType::CatmullRom)
                }
            })
            .collect())
    }

    /// One joint forward pass; the logits of each question's options, in question order.
    fn run(&self, request: &Request) -> Result<(Vec<Vec<f32>>, usize, Timing)> {
        protocol::validate(request, self.codes.len())?;
        if request.questions.len() > MAX_QUESTIONS {
            return Err(
                RequestError(format!("at most {MAX_QUESTIONS} questions per request")).into(),
            );
        }
        let images = self.images(request)?;
        let text = protocol::prompt(request, &self.codes, &self.image_marker, images.len());
        let api = &self.api;
        unsafe {
            let bitmaps: Vec<*mut c_void> = images
                .iter()
                .map(|i| (api.mtmd_bitmap_init)(i.width(), i.height(), i.as_raw().as_ptr()))
                .collect();
            let chunks = (api.mtmd_input_chunks_init)();
            let input = MtmdInputText {
                text: text.as_ptr() as *const c_char,
                text_len: text.len(),
                add_special: false,
                parse_special: true,
            };
            let ptrs: Vec<*const c_void> = bitmaps.iter().map(|b| *b as *const c_void).collect();
            let rc = (api.mtmd_tokenize)(self.mtmd, chunks, &input, ptrs.as_ptr(), ptrs.len());
            let free = |chunks: *mut c_void, bitmaps: &[*mut c_void]| {
                (api.mtmd_input_chunks_free)(chunks);
                for b in bitmaps {
                    (api.mtmd_bitmap_free)(*b);
                }
            };
            if rc != 0 {
                free(chunks, &bitmaps);
                return Err(DeciderError::Run(format!("mtmd_tokenize failed ({rc})")));
            }

            let started = Instant::now();
            (api.llama_memory_clear)((api.llama_get_memory)(self.ctx), true);
            (api.llama_batch_ext_clear)(self.batch);
            let mut keep: Vec<Vec<f32>> = Vec::new(); // image embeddings, alive until decode
            let mut markers: Vec<i32> = Vec::new();
            let mut pos: LlamaPos = 0;
            let mut total = 0usize;
            let mut result: Result<()> = Ok(());
            'chunks: for c in 0..(api.mtmd_input_chunks_size)(chunks) {
                let chunk = (api.mtmd_input_chunks_get)(chunks, c);
                let n = (api.mtmd_input_chunk_get_n_tokens)(chunk);
                total += n;
                if total > self.max_tokens {
                    result = Err(DeciderError::TooLong {
                        tokens: total,
                        limit: self.max_tokens,
                    });
                    break 'chunks;
                }
                match (api.mtmd_input_chunk_get_type)(chunk) {
                    CHUNK_TEXT => {
                        let mut count = 0usize;
                        let tokens = (api.mtmd_input_chunk_get_tokens_text)(chunk, &mut count);
                        for k in 0..count {
                            let tok = *tokens.add(k);
                            let idx = (api.llama_batch_ext_add_token)(self.batch, 0, tok);
                            if idx < 0 {
                                result =
                                    Err(DeciderError::Run(format!("batch add failed ({idx})")));
                                break 'chunks;
                            }
                            (api.llama_batch_ext_set_pos)(self.batch, idx, &pos);
                            if tok == self.marker {
                                (api.llama_batch_ext_set_output_logits)(self.batch, idx, true);
                                markers.push(idx);
                            }
                            pos += 1;
                        }
                    }
                    CHUNK_IMAGE => {
                        let image = (api.mtmd_input_chunk_get_tokens_image)(chunk);
                        let rc = (api.mtmd_encode_chunk)(self.mtmd, chunk);
                        let out = (api.mtmd_get_output_embd)(self.mtmd);
                        if rc != 0 || out.is_null() {
                            result = Err(DeciderError::Run(format!("image encode failed ({rc})")));
                            break 'chunks;
                        }
                        keep.push(std::slice::from_raw_parts(out, n * self.n_embd).to_vec());
                        let embd = keep.last().unwrap();
                        for k in 0..n {
                            let p4: [LlamaPos; 4] = if self.mrope {
                                let d = (api.mtmd_image_tokens_get_decoder_pos)(image, pos, k);
                                [d.t as i32, d.y as i32, d.x as i32, d.z as i32]
                            } else {
                                [pos + k as i32; 4]
                            };
                            let row = LlamaEmbd {
                                data: embd.as_ptr().add(k * self.n_embd),
                                n_rows: 1,
                                n_embd: self.n_embd,
                            };
                            let idx = (api.llama_batch_ext_add_embd)(self.batch, 0, row);
                            if idx < 0 {
                                result =
                                    Err(DeciderError::Run(format!("batch add failed ({idx})")));
                                break 'chunks;
                            }
                            (api.llama_batch_ext_set_pos)(self.batch, idx, p4.as_ptr());
                        }
                        pos += (api.mtmd_input_chunk_get_n_pos)(chunk);
                    }
                    other => {
                        result = Err(DeciderError::Run(format!("unsupported chunk type {other}")));
                        break 'chunks;
                    }
                }
            }
            free(chunks, &bitmaps);
            result?;
            if markers.len() != request.questions.len() {
                return Err(DeciderError::Run(
                    "readout markers do not match questions".into(),
                ));
            }
            let encoded = started.elapsed();
            let started = Instant::now();
            let rc = (api.llama_process)(self.ctx, PROCESS_DECODE, self.batch);
            if rc != 0 {
                return Err(DeciderError::Run(format!("llama_process failed ({rc})")));
            }
            (api.llama_synchronize)(self.ctx);
            let mut out = Vec::new();
            for ((_, question), idx) in request.questions.iter().zip(&markers) {
                let row = (api.llama_get_logits_ith)(self.ctx, *idx);
                if row.is_null() {
                    return Err(DeciderError::Run("no logits at a readout marker".into()));
                }
                let row = std::slice::from_raw_parts(row, self.n_vocab);
                let count = protocol::options(question).0.len();
                out.push(
                    self.code_tokens[..count]
                        .iter()
                        .map(|t| row[*t as usize])
                        .collect(),
                );
            }
            drop(keep);
            Ok((
                out,
                total,
                Timing {
                    tokens: total,
                    encode: encoded,
                    decode: started.elapsed(),
                },
            ))
        }
    }
}
