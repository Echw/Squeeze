use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use optimizer_core::{
    CompressionProfile, NeverCancelled, NoProgress, OptimizationReport, OptimizeOptions, optimize,
};

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
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProfileArg {
    MaximumQuality,
    Balanced,
    MaximumCompression,
    Lossless,
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
            json,
        } => optimize_file(&input, output.as_deref(), profile.into(), json),
        Command::Benchmark {
            directory,
            csv,
            json,
            profile,
        } => benchmark(&directory, profile.into(), csv, json),
    }
}

fn optimize_file(
    input_path: &Path,
    output_path: Option<&Path>,
    profile: CompressionProfile,
    json: bool,
) -> Result<()> {
    let input =
        fs::read(input_path).with_context(|| format!("cannot read {}", input_path.display()))?;
    let options = OptimizeOptions {
        profile,
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

fn benchmark(directory: &Path, profile: CompressionProfile, csv: bool, json: bool) -> Result<()> {
    if !directory.is_dir() {
        bail!("{} is not a directory", directory.display());
    }
    let mut paths = Vec::new();
    collect_images(directory, &mut paths)?;
    paths.sort();
    let mut reports: Vec<(String, OptimizationReport)> = Vec::new();
    for path in paths {
        let input = fs::read(&path)?;
        let options = OptimizeOptions {
            profile,
            ..OptimizeOptions::default()
        };
        match optimize(&input, options, &NoProgress, &NeverCancelled) {
            Ok(result) => reports.push((path.display().to_string(), result.report)),
            Err(error) => eprintln!("{}: {error}", path.display()),
        }
    }
    if csv {
        println!(
            "file,format,original_bytes,optimized_bytes,saved_percent,ssimulacra2,butteraugli,candidates,time_ms"
        );
        for (path, report) in reports {
            println!(
                "\"{}\",{:?},{},{},{:.4},{},{},{},{:.3}",
                path.replace('"', "\"\""),
                report.format,
                report.original_size,
                report.optimized_size,
                report.saved_percent,
                optional_number(report.metrics.ssimulacra2),
                optional_number(report.metrics.butteraugli),
                report.candidates_tested,
                report.processing_time_ms,
            );
        }
    } else if json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        for (path, report) in reports {
            println!(
                "{path}: {:.1}% saved in {:.0} ms",
                report.saved_percent, report.processing_time_ms
            );
        }
    }
    io::stdout().flush()?;
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
