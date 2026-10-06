use js_sys::Function;
use optimizer_core::{
    CancellationToken, NeverCancelled, OptimizationObserver, OptimizationOperation,
    OptimizeOptions, ProgressEvent, ProgressSink, optimize, optimize_with_observer,
};
use wasm_bindgen::prelude::*;

const WORKER_API_VERSION: u8 = 6;

struct JsProgress<'a>(&'a Function);

struct JsObserver<'a>(&'a Function);

impl ProgressSink for JsProgress<'_> {
    fn report(&self, event: ProgressEvent) {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = self.0.call1(&JsValue::NULL, &JsValue::from_str(&json));
        }
    }
}

impl OptimizationObserver for JsObserver<'_> {
    fn begin(&self, operation: OptimizationOperation) {
        self.emit("begin", operation);
    }
    fn end(&self, operation: OptimizationOperation) {
        self.emit("end", operation);
    }
}

impl JsObserver<'_> {
    fn emit(&self, kind: &str, operation: OptimizationOperation) {
        let payload = serde_json::json!({ "type": kind, "operation": operation });
        let _ = self
            .0
            .call1(&JsValue::NULL, &JsValue::from_str(&payload.to_string()));
    }
}

#[wasm_bindgen]
pub struct WasmOptimizationResult {
    report_json: String,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl WasmOptimizationResult {
    #[wasm_bindgen(getter)]
    pub fn report_json(&self) -> String {
        self.report_json.clone()
    }

    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

#[wasm_bindgen]
pub fn worker_api_version() -> u8 {
    WORKER_API_VERSION
}

#[wasm_bindgen]
pub fn optimize_image(
    input: &[u8],
    options_json: &str,
    progress: &Function,
) -> Result<WasmOptimizationResult, JsValue> {
    let options: OptimizeOptions = serde_json::from_str(options_json)
        .map_err(|error| js_error("INVALID_OPTIONS", &error.to_string(), true))?;
    let sink = JsProgress(progress);
    let cancellation: &dyn CancellationToken = &NeverCancelled;
    let started = js_sys::Date::now();
    let mut result = optimize(input, options, &sink, cancellation).map_err(|error| {
        let (code, recoverable) = match &error {
            optimizer_core::OptimizeError::Cancelled => ("CANCELLED", true),
            optimizer_core::OptimizeError::UnsupportedFormat => ("UNSUPPORTED_FORMAT", true),
            optimizer_core::OptimizeError::AnimatedPng => ("ANIMATED_PNG", true),
            optimizer_core::OptimizeError::InputTooLarge { .. }
            | optimizer_core::OptimizeError::PixelLimit { .. }
            | optimizer_core::OptimizeError::MemoryLimit { .. } => ("RESOURCE_LIMIT", true),
            optimizer_core::OptimizeError::InvalidColorProfile => ("INVALID_COLOR_PROFILE", true),
            optimizer_core::OptimizeError::Decode(_) => ("DECODE_FAILED", true),
            optimizer_core::OptimizeError::Encode(_) => ("ENCODE_FAILED", true),
            optimizer_core::OptimizeError::Metric(_) => ("METRIC_FAILED", true),
        };
        js_error(code, &error.to_string(), recoverable)
    })?;
    result.report.processing_time_ms = js_sys::Date::now() - started;
    let report_json = serde_json::to_string(&result.report)
        .map_err(|error| js_error("SERIALIZE_FAILED", &error.to_string(), false))?;
    Ok(WasmOptimizationResult {
        report_json,
        bytes: result.output,
    })
}

/// Scores JPEG-encoded tiles for the Worker's JPEG quality search.
#[wasm_bindgen]
#[derive(Default)]
pub struct JpegTileScorer(optimizer_core::JpegTileScorer);

#[wasm_bindgen]
impl JpegTileScorer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_tile(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<usize, JsValue> {
        self.0.add_tile(rgba, width, height).map_err(map_error)
    }

    pub fn score(&mut self, index: usize, jpeg: &[u8]) -> Result<f64, JsValue> {
        self.0.score(index, jpeg).map_err(map_error)
    }
}

/// Diagnostic variant used only by the benchmark Worker. Its callback receives
/// phase boundaries; production requests keep using `optimize_image`.
#[wasm_bindgen]
pub fn optimize_image_with_diagnostics(
    input: &[u8],
    options_json: &str,
    progress: &Function,
    diagnostics: &Function,
) -> Result<WasmOptimizationResult, JsValue> {
    let options: OptimizeOptions = serde_json::from_str(options_json)
        .map_err(|error| js_error("INVALID_OPTIONS", &error.to_string(), true))?;
    let sink = JsProgress(progress);
    let observer = JsObserver(diagnostics);
    let cancellation: &dyn CancellationToken = &NeverCancelled;
    let started = js_sys::Date::now();
    let mut result = optimize_with_observer(input, options, &sink, cancellation, &observer)
        .map_err(map_error)?;
    result.report.processing_time_ms = js_sys::Date::now() - started;
    let report_json = serde_json::to_string(&result.report)
        .map_err(|error| js_error("SERIALIZE_FAILED", &error.to_string(), false))?;
    Ok(WasmOptimizationResult {
        report_json,
        bytes: result.output,
    })
}

fn map_error(error: optimizer_core::OptimizeError) -> JsValue {
    let (code, recoverable) = match &error {
        optimizer_core::OptimizeError::Cancelled => ("CANCELLED", true),
        optimizer_core::OptimizeError::UnsupportedFormat => ("UNSUPPORTED_FORMAT", true),
        optimizer_core::OptimizeError::AnimatedPng => ("ANIMATED_PNG", true),
        optimizer_core::OptimizeError::InputTooLarge { .. }
        | optimizer_core::OptimizeError::PixelLimit { .. }
        | optimizer_core::OptimizeError::MemoryLimit { .. } => ("RESOURCE_LIMIT", true),
        optimizer_core::OptimizeError::InvalidColorProfile => ("INVALID_COLOR_PROFILE", true),
        optimizer_core::OptimizeError::Decode(_) => ("DECODE_FAILED", true),
        optimizer_core::OptimizeError::Encode(_) => ("ENCODE_FAILED", true),
        optimizer_core::OptimizeError::Metric(_) => ("METRIC_FAILED", true),
    };
    js_error(code, &error.to_string(), recoverable)
}

fn js_error(code: &str, message: &str, recoverable: bool) -> JsValue {
    let body = serde_json::json!({
        "code": code,
        "message": message,
        "recoverable": recoverable,
    });
    JsValue::from_str(&body.to_string())
}
