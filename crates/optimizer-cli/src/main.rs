use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use optimizer_core::{
    CompressionMethod, CompressionProfile, NeverCancelled, NoProgress, OptimizationObserver,
    OptimizationOperation, OptimizationReport, OptimizeOptions, QualityMetrics, SearchEffort,
    measure_quality, optimize, optimize_with_observer,
};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "optimizer",
    version,
    about = "Local JPEG/PNG perceptual optimizer"
)]
struct App {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Optimize {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = ProfileArg::Balanced)]
        profile: ProfileArg,
        #[arg(long, value_enum, default_value_t = MethodArg::Auto)]
        method: MethodArg,
        #[arg(long, value_enum, default_value_t = EffortArg::Auto)]
        search_effort: EffortArg,
        #[arg(long)]
        json: bool,
    },
    Benchmark {
        directory: PathBuf,
        #[arg(long, conflicts_with = "json")]
        csv: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, value_enum, default_value_t = ProfileArg::Balanced)]
        profile: ProfileArg,
        #[arg(long, value_enum, default_value_t = MethodArg::Auto)]
        method: MethodArg,
        #[arg(long, value_enum, default_value_t = EffortArg::Auto)]
        search_effort: EffortArg,
        #[arg(long, default_value_t = 5)]
        runs: u16,
        #[arg(long, default_value_t = 1)]
        warmup: u16,
    },
    /// Create deterministic, permissively-generated images outside the repo.
    Fixtures { output: PathBuf },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProfileArg {
    MaximumQuality,
    Balanced,
    MaximumCompression,
    Lossless,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MethodArg {
    Auto,
    Search,
    Lossless,
    Palette,
}
impl From<MethodArg> for CompressionMethod {
    fn from(value: MethodArg) -> Self {
        match value {
            MethodArg::Auto => Self::Auto,
            MethodArg::Search => Self::Search,
            MethodArg::Lossless => Self::Lossless,
            MethodArg::Palette => Self::Palette,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EffortArg {
    Auto,
    Detailed,
}
impl From<EffortArg> for SearchEffort {
    fn from(value: EffortArg) -> Self {
        match value {
            EffortArg::Auto => Self::Auto,
            EffortArg::Detailed => Self::Detailed,
        }
    }
}

impl From<ProfileArg> for CompressionProfile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::MaximumQuality => Self::MaximumQuality,
            ProfileArg::Balanced => Self::Balanced,
            ProfileArg::MaximumCompression => Self::MaximumCompression,
            ProfileArg::Lossless => Self::Lossless,
        }
    }
}

fn main() -> Result<()> {
    match App::parse().command {
        Command::Optimize {
            input,
            output,
            profile,
            method,
            search_effort,
            json,
        } => optimize_file(
            &input,
            output.as_deref(),
            profile.into(),
            method.into(),
            search_effort.into(),
            json,
        ),
        Command::Benchmark {
            directory,
            csv,
            json,
            profile,
            method,
            search_effort,
            runs,
            warmup,
        } => benchmark(
            &directory,
            BenchmarkConfig {
                profile: profile.into(),
                method: method.into(),
                search_effort: search_effort.into(),
                runs,
                warmup,
            },
            csv,
            json,
        ),
        Command::Fixtures { output } => write_fixtures(&output),
    }
}

fn optimize_file(
    input_path: &Path,
    output_path: Option<&Path>,
    profile: CompressionProfile,
    method: CompressionMethod,
    search_effort: SearchEffort,
    json: bool,
) -> Result<()> {
    let input =
        fs::read(input_path).with_context(|| format!("cannot read {}", input_path.display()))?;
    let options = OptimizeOptions {
        profile,
        method,
        search_effort,
        ..OptimizeOptions::default()
    };
    let result = optimize(&input, options, &NoProgress, &NeverCancelled)?;
    let destination = output_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_name(input_path));
    fs::write(&destination, &result.output)
        .with_context(|| format!("cannot write {}", destination.display()))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result.report)?);
    } else if result.report.already_optimized {
        println!("Already optimized → {}", destination.display());
    } else {
        println!(
            "Saved {:.1}% ({} → {} bytes) → {}",
            result.report.saved_percent,
            result.report.original_size,
            result.report.optimized_size,
            destination.display()
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct BenchmarkConfig {
    profile: CompressionProfile,
    method: CompressionMethod,
    search_effort: SearchEffort,
    runs: u16,
    warmup: u16,
}

fn benchmark(directory: &Path, config: BenchmarkConfig, csv: bool, json: bool) -> Result<()> {
    if !directory.is_dir() {
        bail!("{} is not a directory", directory.display());
    }
    let mut paths = Vec::new();
    collect_images(directory, &mut paths)?;
    paths.sort();
    let mut reports: Vec<(String, BenchmarkReport)> = Vec::new();
    for path in paths {
        let input = fs::read(&path)?;
        let options = OptimizeOptions {
            profile: config.profile,
            method: config.method,
            search_effort: config.search_effort,
            ..OptimizeOptions::default()
        };
        match benchmark_one(&input, options, config.runs, config.warmup) {
            Ok(result) => reports.push((path.display().to_string(), result)),
            Err(error) => eprintln!("{}: {error}", path.display()),
        }
    }
    if csv {
        println!(
            "file,format,original_bytes,optimized_bytes,saved_percent,ssimulacra2,butteraugli,candidates,median_ms,p95_ms"
        );
        for (path, report) in reports {
            println!(
                "\"{}\",{:?},{},{},{:.4},{},{},{},{:.3},{:.3}",
                path.replace('"', "\"\""),
                report.report.format,
                report.report.original_size,
                report.report.optimized_size,
                report.report.saved_percent,
                optional_number(report.report.metrics.ssimulacra2),
                optional_number(report.report.metrics.butteraugli),
                report.report.candidates_tested,
                report.median_ms,
                report.p95_ms,
            );
        }
    } else if json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        for (path, report) in reports {
            println!(
                "{path}: {:.1}% saved in {:.0} ms",
                report.report.saved_percent, report.median_ms
            );
        }
    }
    io::stdout().flush()?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    report: OptimizationReport,
    oracle_metrics: QualityMetrics,
    median_ms: f64,
    p95_ms: f64,
    operation_ms: BTreeMap<String, f64>,
    peak_rss_bytes: Option<u64>,
}

#[derive(Default)]
struct TimingObserver {
    started: RefCell<BTreeMap<OptimizationOperation, Instant>>,
    totals: RefCell<BTreeMap<OptimizationOperation, f64>>,
}
impl OptimizationObserver for TimingObserver {
    fn begin(&self, operation: OptimizationOperation) {
        self.started.borrow_mut().insert(operation, Instant::now());
    }
    fn end(&self, operation: OptimizationOperation) {
        if let Some(start) = self.started.borrow_mut().remove(&operation) {
            *self.totals.borrow_mut().entry(operation).or_default() +=
                start.elapsed().as_secs_f64() * 1000.0;
        }
    }
}
fn benchmark_one(
    input: &[u8],
    options: OptimizeOptions,
    runs: u16,
    warmup: u16,
) -> Result<BenchmarkReport> {
    for _ in 0..warmup {
        let _ = optimize(input, options, &NoProgress, &NeverCancelled)?;
    }
    let mut durations = Vec::new();
    let mut report = None;
    let mut output = None;
    let mut sums: BTreeMap<String, f64> = BTreeMap::new();
    for _ in 0..runs.max(1) {
        let observer = TimingObserver::default();
        let started = Instant::now();
        let result =
            optimize_with_observer(input, options, &NoProgress, &NeverCancelled, &observer)?;
        durations.push(started.elapsed().as_secs_f64() * 1000.0);
        output = Some(result.output);
        report = Some(result.report);
        for (operation, ms) in observer.totals.into_inner() {
            *sums.entry(format!("{operation:?}")).or_default() += ms;
        }
    }
    durations.sort_by(f64::total_cmp);
    let count = durations.len();
    Ok(BenchmarkReport {
        report: report.expect("at least one benchmark run"),
        oracle_metrics: measure_quality(input, &output.expect("at least one benchmark output"))?,
        median_ms: durations[count / 2],
        p95_ms: durations[((count - 1) * 95) / 100],
        operation_ms: sums
            .into_iter()
            .map(|(name, sum)| (name, sum / count as f64))
            .collect(),
        peak_rss_bytes: peak_rss_bytes(),
    })
}

#[cfg(not(target_os = "windows"))]
fn peak_rss_bytes() -> Option<u64> {
    // `getrusage` returns the high-water mark for this process. macOS reports
    // bytes; Linux and the BSD variants used in CI report KiB.
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: `usage` points to valid, writable storage for the platform C ABI.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: a zero return from getrusage initializes the provided rusage.
    let maximum = unsafe { usage.assume_init() }.ru_maxrss.max(0) as u64;
    #[cfg(target_os = "macos")]
    {
        Some(maximum)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(maximum.saturating_mul(1024))
    }
}

// Windows does not provide POSIX getrusage. Keep benchmarking functional; a
// future Windows-specific process-memory adapter can populate this field.
#[cfg(target_os = "windows")]
fn peak_rss_bytes() -> Option<u64> {
    None
}

fn write_fixtures(output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;
    write_png(
        &output.join("squeeze-fixture-1317x1030.png"),
        1317,
        1030,
        false,
    )?;
    write_png(&output.join("sparse-alpha.png"), 128, 96, true)?;
    println!(
        "Generated legal deterministic fixtures in {}",
        output.display()
    );
    Ok(())
}
fn write_png(path: &Path, width: u32, height: u32, sparse_alpha: bool) -> Result<()> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            // Deterministic flat areas, gradients, fine edges and low-amplitude
            // noise. It is generated here, so it is safe to publish and use in
            // a regression corpus without importing a private image.
            let field = ((x / 64 + y / 48) % 3) as u8;
            let noise = x
                .wrapping_mul(73_856_093)
                .wrapping_add(y.wrapping_mul(19_349_663))
                .rotate_left((x % 17) + 1);
            pixels.extend_from_slice(&[
                72_u8
                    .saturating_add(field * 38)
                    .saturating_add((noise & 7) as u8),
                94_u8
                    .saturating_add(field * 28)
                    .saturating_add(((noise >> 3) & 7) as u8),
                54_u8
                    .saturating_add(field * 19)
                    .saturating_add(((noise >> 6) & 7) as u8),
                if sparse_alpha && x == width - 1 && y == height - 1 {
                    1
                } else {
                    255
                },
            ]);
        }
    }
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}

fn collect_images(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_images(&path, output)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "jpg" | "jpeg" | "png"
                )
            })
        {
            output.push(path);
        }
    }
    Ok(())
}

fn output_name(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jpg");
    path.with_file_name(format!("{stem}.optimized.{extension}"))
}

fn optional_number(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.5}"))
        .unwrap_or_default()
}
