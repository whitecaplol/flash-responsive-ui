#![feature(iter_advance_by)]
#![feature(iter_intersperse)]

use crate::{analyze::create_docs, normalize::Normalize, url::UrlPath};
use clap::Parser;
use config::Config;
use log::{error, info};
use std::{error::Error, fs, path::PathBuf, process::exit, time::Instant};

mod analyze;
mod annotation;
mod builder;
mod cmake;
mod config;
mod html;
mod lookahead;
mod normalize;
mod url;

#[derive(Parser, Debug)]
#[command(name("Flash"), version, about)]
struct Args {
    /// Input directory with the flash.json file
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory where to place the generated docs
    #[arg(short, long)]
    output: PathBuf,

    /// Whether to overwrite output directory if it already exists
    #[arg(long, default_value_t = false)]
    overwrite: bool,

    /// Whether to skip invoking CMake entirely, relies on existing build dir.
    #[arg(long, default_value_t = false, hide = true)]
    skip_build: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    FlashLogger::init();

    let args = Args::parse();

    // Check if output dir exists
    if args.output.exists()
        // Check if it's empty
        && args.output.read_dir().map(|mut i| i.next().is_some()).unwrap_or(false)
        // Then overwrite must be specified
        && !args.overwrite
    {
        error!(
            "Output directory {} already exists and no --overwrite option was specified, aborting",
            args.output.to_string_lossy()
        );
        exit(1);
    }

    if !args.output.exists() {
        fs::create_dir_all(&args.output)?;
    }

    let relative_output = if args.output.is_relative() {
        Some(UrlPath::try_from(&args.output).ok()).flatten()
    } else {
        None
    };

    // Relink working directory to input dir and use absolute path for output
    // Not using fs::canonicalize because that returns UNC paths on Windows and
    // those break things
    let full_output = if args.output.is_absolute() {
        args.output
    } else {
        std::env::current_dir()?.join(args.output).normalize()
    };
    let full_input = if args.input.is_absolute() {
        args.input
    } else {
        std::env::current_dir()?.join(args.input).normalize()
    };
    std::env::set_current_dir(&full_input).expect(
        "Unable to set input dir as working directory \
            (probable reason is it doesn't exist)",
    );

    // Parse config
    let conf = Config::parse(full_input, full_output, relative_output)?;

    // Build the docs
    info!(
        "Building docs for {} ({})",
        conf.project.name, conf.project.version
    );
    let now = Instant::now();
    create_docs(conf.clone(), args.skip_build).await?;
    info!(
        "Docs built for {} in {}s",
        conf.project.name,
        now.elapsed().as_secs()
    );

    Ok(())
}

struct FlashLogger;

impl log::Log for FlashLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        use log::Level;
        use owo_colors::OwoColorize;
        let header = match record.level() {
            Level::Warn => "[warn]".yellow().bold().to_string(),
            Level::Error => "[error]".red().bold().to_string(),
            Level::Info => "[info]".bright_blue().bold().to_string(),
            Level::Debug | Level::Trace => "[debug]".bright_purple().bold().to_string(),
        };
        println!("{} {}", header, record.args());
    }

    fn flush(&self) {}
}

static LOGGER: FlashLogger = FlashLogger;
impl FlashLogger {
    pub fn init() {
        log::set_logger(&LOGGER).expect("Failed to initialize logger");
        log::set_max_level(if cfg!(debug_assertions) {
            log::LevelFilter::Trace
        } else {
            log::LevelFilter::Info
        });
    }
}
