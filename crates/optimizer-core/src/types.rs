use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLimits {
    pub max_input_bytes: usize,
    pub max_pixels: u64,
    pub max_working_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 100 * 1024 * 1024,
            max_pixels: 24_000_000,
            max_working_bytes: 768 * 1024 * 1024,
        }
    }
}

/// Squeeze deliberately exposes no compression knobs. The engine always keeps
/// the input format and chooses one fixed, safe encoder configuration.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OptimizeOptions {
    pub limits: ResourceLimits,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Jpeg,
    Png,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProgressStage {
    Decoding,
    Analyzing,
    Compressing,
    Finalizing,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub stage: ProgressStage,
}

pub trait ProgressSink {
    fn report(&self, event: ProgressEvent);
}

/// Optional timing events for the benchmark command. Production callers do
/// not retain telemetry or execute additional quality measurements.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum OptimizationOperation {
    Decode,
    Analysis,
    JpegEncode,
    PaletteBuild,
    PngEncode,
    LosslessRecompress,
    LosslessVerification,
}

pub trait OptimizationObserver {
    fn begin(&self, operation: OptimizationOperation);
    fn end(&self, operation: OptimizationOperation);
}

pub trait CancellationToken {
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn report(&self, _event: ProgressEvent) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoObserver;

impl OptimizationObserver for NoObserver {
    fn begin(&self, _operation: OptimizationOperation) {}
    fn end(&self, _operation: OptimizationOperation) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NeverCancelled;

impl CancellationToken for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Only the benchmark command populates these values. Daily compression does
/// not calculate perceptual metrics in the browser.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityMetrics {
    pub ssimulacra2: Option<f64>,
    pub butteraugli: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAnalysis {
    pub kind: ContentKind,
    pub entropy: f32,
    pub estimated_colors: u32,
    pub edge_density: f32,
    pub noise: f32,
    pub flat_area_ratio: f32,
    pub has_alpha: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Photo,
    Graphic,
    Screenshot,
    Mixed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedStrategy {
    pub encoder: String,
    pub quality: Option<u8>,
    pub chroma_subsampling: Option<String>,
    pub progressive: Option<bool>,
    pub palette_colors: Option<u16>,
    pub lossless: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationReport {
    pub format: ImageFormat,
    pub output_format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub original_size: usize,
    pub optimized_size: usize,
    pub saved_bytes: usize,
    pub saved_percent: f32,
    pub strategy: SelectedStrategy,
    pub processing_time_ms: f64,
    pub already_optimized: bool,
    pub optimizer_version: u16,
    pub warnings: Vec<String>,
    pub analysis: ImageAnalysis,
}

#[derive(Clone, Debug)]
pub struct OptimizationResult {
    pub output: Vec<u8>,
    pub report: OptimizationReport,
}

#[derive(Debug, thiserror::Error)]
pub enum OptimizeError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("unsupported format; only JPEG and PNG are accepted")]
    UnsupportedFormat,
    #[error("animated PNG is not supported")]
    AnimatedPng,
    #[error("input exceeds the {limit} byte limit")]
    InputTooLarge { limit: usize },
    #[error("image has {pixels} pixels; limit is {limit}")]
    PixelLimit { pixels: u64, limit: u64 },
    #[error("estimated working memory {estimated} bytes exceeds limit {limit}")]
    MemoryLimit { estimated: u64, limit: u64 },
    #[error("image decoding failed: {0}")]
    Decode(String),
    #[error("image encoding failed: {0}")]
    Encode(String),
    #[error("quality measurement failed: {0}")]
    Metric(String),
    #[error("unsupported or malformed color profile")]
    InvalidColorProfile,
}

pub(crate) const OPTIMIZER_VERSION: u16 = 5;
