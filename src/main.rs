use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::sync::Arc;

use clap::Parser;
use thiserror::Error;

mod claim_parser;
mod compression;
mod filter;
mod json;
mod ntriples;
mod rdf;

use compression::{
    create_compressed_writer, create_input_reader, determine_compression, OUTPUT_BUFFER_SIZE,
};
use filter::EntityFilter;
use json::filter_json_parallel;
use rdf::{filter_rdf_parallel, OutputFormat};

#[derive(Parser, Debug)]
#[command(name = "wikidata-werkzeug")]
#[command(author, version, about = "Filter Wikidata dumps (RDF truthy and JSON formats)", long_about = None)]
struct Args {
    /// Filter by claim (e.g., P31:Q5, P31:Q5,Q6256, P31:Q5&P18)
    /// Supports: AND (&), OR (|, or comma for values), NOT (~)
    #[arg(short, long)]
    claim: Option<String>,

    /// Entity type to filter: item, property, or both
    #[arg(short = 't', long, default_value = "item")]
    r#type: String,

    /// Input format: auto, rdf, json (auto-detects from extension/content)
    #[arg(short = 'f', long, default_value = "auto")]
    format: String,

    /// Output format: same (preserve input format), ntriples, json
    #[arg(short = 'o', long, default_value = "same")]
    output_format: String,

    /// Filter languages for labels/descriptions (comma-separated, e.g., en,de,fr)
    #[arg(short = 'l', long)]
    languages: Option<String>,

    /// Exclude language subvariants (e.g., de will NOT include de-ch, de-at)
    #[arg(long, default_value = "false")]
    language_exact_match: bool,

    /// Input file (stdin if not provided, supports .bz2, .gz, .lz4)
    #[arg()]
    input: Option<String>,

    /// Output file (stdout if not provided). Extension determines compression (.gz, .lz4)
    #[arg(long)]
    output: Option<String>,

    /// Output compression: none, gzip, lz4 (auto-detected from --output extension)
    #[arg(long, default_value = "none")]
    compress: String,

    /// Show progress info on stderr
    #[arg(short = 'p', long)]
    progress: bool,

    /// Keep only specified subject entity IDs (comma-separated, e.g., Q31,Q42)
    #[arg(long)]
    subject: Option<String>,

    /// Keep only triples with specified properties (comma-separated, e.g., P31,P279)
    #[arg(long)]
    property: Option<String>,

    /// Number of threads for parallel processing (default: number of CPUs)
    #[arg(long)]
    threads: Option<usize>,

    /// Keep only specified entity attributes (comma-separated)
    /// Valid attributes: id, type, labels, descriptions, aliases, claims, sitelinks
    #[arg(long)]
    keep: Option<String>,

    /// Omit specified entity attributes (comma-separated)
    /// Valid attributes: id, type, labels, descriptions, aliases, claims, sitelinks
    #[arg(long)]
    omit: Option<String>,

    /// Batch size for parallel processing (default: 1000 for JSON, 100 for RDF)
    #[arg(long)]
    batch_size: Option<usize>,

    /// Skip the first N lines before processing (useful for resuming interrupted jobs)
    #[arg(long, default_value = "0")]
    skip_lines: u64,

    /// Stop processing after N lines (0 = no limit)
    #[arg(long, default_value = "0")]
    max_lines: u64,
}

#[derive(Error, Debug)]
pub enum FilterError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid claim filter: {0}")]
    InvalidClaim(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

fn main() -> Result<(), FilterError> {
    let args = Args::parse();

    // Configure rayon thread pool if specified
    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .ok();
    }

    // Build filters
    let claim_filter = if let Some(ref claim_str) = args.claim {
        Some(claim_parser::parse_claim_filter(claim_str)?)
    } else {
        None
    };

    let subject_filter: Option<HashSet<String>> = args
        .subject
        .as_ref()
        .map(|s| s.split(',').map(|id| id.trim().to_string()).collect());

    let property_filter: Option<HashSet<String>> = args
        .property
        .as_ref()
        .map(|s| s.split(',').map(|id| id.trim().to_string()).collect());

    let language_filter: Option<HashSet<String>> = args
        .languages
        .as_ref()
        .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());

    // Parse keep/omit attribute filters
    let (keep_attributes, omit_attributes) =
        filter::parse_attribute_filters(args.keep.as_deref(), args.omit.as_deref())?;

    let entity_filter = Arc::new(EntityFilter {
        claim_filter,
        subject_filter,
        property_filter,
        language_filter,
        language_include_subvariants: !args.language_exact_match,
        entity_type: args.r#type.clone(),
        keep_attributes,
        omit_attributes,
    });

    // Determine input format and create reader
    let (reader, detected_format): (Box<dyn BufRead + Send>, String) = match &args.input {
        Some(path) => create_input_reader(path, &args.format)?,
        None => {
            let stdin = io::stdin();
            let format = if args.format == "auto" {
                "rdf".to_string()
            } else {
                args.format.clone()
            };
            (Box::new(BufReader::new(stdin)), format)
        }
    };

    // Determine compression from --compress or output file extension
    let compression = determine_compression(&args.compress, args.output.as_deref());

    let skip_lines = args.skip_lines;
    let max_lines = if args.max_lines == 0 {
        u64::MAX
    } else {
        args.max_lines
    };

    if skip_lines > 0 && args.progress {
        eprintln!("Skipping first {} lines...", skip_lines);
    }

    // Determine output format
    let output_format = match args.output_format.as_str() {
        "json" => OutputFormat::Json,
        "ntriples" => OutputFormat::NTriples,
        "same" => {
            // Preserve input format
            match detected_format.as_str() {
                "json" | "ndjson" => OutputFormat::Json,
                _ => OutputFormat::NTriples,
            }
        }
        _ => OutputFormat::NTriples,
    };

    // Create output writer with optional compression
    let output_writer: Box<dyn Write> = match &args.output {
        Some(path) => {
            let file = std::fs::File::create(path)?;
            create_compressed_writer(file, &compression)
        }
        None => {
            let stdout = io::stdout();
            create_compressed_writer(stdout, &compression)
        }
    };

    let mut output = BufWriter::with_capacity(OUTPUT_BUFFER_SIZE, output_writer);

    match detected_format.as_str() {
        "rdf" | "ntriples" | "nt" => {
            let batch_size = args.batch_size.unwrap_or(100);
            filter_rdf_parallel(
                reader,
                &mut output,
                &entity_filter,
                args.progress,
                batch_size,
                skip_lines,
                max_lines,
                output_format,
            )?;
        }
        "json" | "ndjson" => {
            let batch_size = args.batch_size.unwrap_or(1000);
            filter_json_parallel(
                reader,
                &mut output,
                &entity_filter,
                args.progress,
                batch_size,
                skip_lines,
                max_lines,
                output_format,
            )?;
        }
        _ => {
            eprintln!("Unknown format: {}, assuming RDF", detected_format);
            let batch_size = args.batch_size.unwrap_or(100);
            filter_rdf_parallel(
                reader,
                &mut output,
                &entity_filter,
                args.progress,
                batch_size,
                skip_lines,
                max_lines,
                output_format,
            )?;
        }
    }

    // Flush the buffered writer
    output.flush()?;

    // For LZ4, we need to finish the encoder to write the frame footer
    // This is handled by dropping the writer, but we should explicitly flush
    drop(output);

    Ok(())
}
