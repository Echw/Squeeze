use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompressionProfile {
    MaximumQuality,
    #[default]
    Balanced,
    MaximumCompression,
    Lossless,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    #[default]
    Preserve,
    Webp,
    Avif,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchEffort {
    #[default]
    Auto,
    Detailed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompressionMethod {
    /// Evaluate every compatible strategy and keep the smallest safe result.
    #[default]
    Auto,
    /// Only recompress the existing PNG stream; decoded pixels never change.
    Lossless,
    /// Only try indexed-palette candidates under the selected quality threshold.
    Palette,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MetadataPolicy {
    #[default]
    StripPrivate,
    PreserveAll,
}

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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OptimizeOptions {
    pub profile: CompressionProfile,
    pub search_effort: SearchEffort,
    pub method: CompressionMethod,
    pub output_format: OutputFormat,
    pub metadata: MetadataPolicy,
    pub limits: ResourceLimits,
}

impl Default for OptimizeOptions {
    fn default() -> Self {
        Self {
            profile: CompressionProfile::Balanced,
            search_effort: SearchEffort::Auto,
            method: CompressionMethod::Auto,
            output_format: OutputFormat::Preserve,
            metadata: MetadataPolicy::StripPrivate,
            limits: ResourceLimits::default(),
        }
    }
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
    Searching,
    Measuring,
    Finalizing,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub stage: ProgressStage,
    pub candidate: Option<u16>,
    pub total: Option<u16>,
}

pub trait ProgressSink {
    fn report(&self, event: ProgressEvent);
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
pub struct NeverCancelled;

impl CancellationToken for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

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
    pub dithering: Option<String>,
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
    pub metrics: QualityMetrics,
    pub strategy: SelectedStrategy,
    pub candidates_tested: u16,
    pub processing_time_ms: f64,
    pub already_optimized: bool,
    pub profile_set_version: u16,
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
    #[error("requested output format is not built into this engine")]
    UnsupportedOutputFormat,
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfileRules {
    pub ssimulacra2: f64,
    pub butteraugli: f64,
    pub candidate_budget: u16,
    pub start_quality: u8,
}

pub(crate) const PROFILE_SET_VERSION: u16 = 2;

impl CompressionProfile {
    pub(crate) fn rules(self, effort: SearchEffort) -> Option<ProfileRules> {
        let detailed = matches!(effort, SearchEffort::Detailed);
        match self {
            Self::MaximumQuality => Some(ProfileRules {
                ssimulacra2: 99.0,
                butteraugli: 1.0,
                candidate_budget: if detailed { 24 } else { 8 },
                start_quality: 96,
            }),
            Self::Balanced => Some(ProfileRules {
                ssimulacra2: 97.0,
                butteraugli: 1.5,
                candidate_budget: if detailed { 36 } else { 12 },
                start_quality: 92,
            }),
            Self::MaximumCompression => Some(ProfileRules {
                ssimulacra2: 93.0,
                butteraugli: 2.0,
                candidate_budget: if detailed { 48 } else { 16 },
                start_quality: 86,
            }),
            Self::Lossless => None,
        }
    }
}
