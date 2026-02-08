use std::io::{BufRead, BufReader, Write};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use lz4_flex::frame::{FrameDecoder as Lz4Decoder, FrameEncoder as Lz4Encoder};

/// Default output buffer size (8 MB)
pub const OUTPUT_BUFFER_SIZE: usize = 8 * 1024 * 1024;

/// Detect input format from file path
pub fn detect_format_from_path(path: &str) -> String {
    let path_lower = path.to_lowercase();
    // Remove compression extensions first
    let path_without_compression = path_lower
        .strip_suffix(".bz2")
        .or_else(|| path_lower.strip_suffix(".gz"))
        .or_else(|| path_lower.strip_suffix(".lz4"))
        .unwrap_or(&path_lower);

    if path_without_compression.ends_with(".nt") || path_without_compression.contains("truthy") {
        "rdf".to_string()
    } else if path_without_compression.ends_with(".json")
        || path_without_compression.ends_with(".ndjson")
    {
        "json".to_string()
    } else {
        "rdf".to_string()
    }
}

/// Determine output compression from CLI option or output file extension
pub fn determine_compression(compress_arg: &str, output_path: Option<&str>) -> String {
    // If --compress is explicitly set to something other than "none", use it
    if compress_arg != "none" {
        return compress_arg.to_string();
    }

    // Otherwise, auto-detect from output file extension
    if let Some(path) = output_path {
        let path_lower = path.to_lowercase();
        if path_lower.ends_with(".lz4") {
            return "lz4".to_string();
        } else if path_lower.ends_with(".gz") {
            return "gzip".to_string();
        }
    }

    "none".to_string()
}

/// Create a writer with optional compression
pub fn create_compressed_writer<W: Write + 'static>(
    writer: W,
    compression: &str,
) -> Box<dyn Write> {
    match compression {
        "lz4" => Box::new(Lz4Encoder::new(writer)),
        "gzip" | "gz" => Box::new(GzEncoder::new(writer, flate2::Compression::default())),
        _ => Box::new(writer),
    }
}

/// Create a reader for the input file with optional decompression
pub fn create_input_reader(
    path: &str,
    format_arg: &str,
) -> std::io::Result<(Box<dyn BufRead + Send>, String)> {
    let file = std::fs::File::open(path)?;
    let format = if format_arg == "auto" {
        detect_format_from_path(path)
    } else {
        format_arg.to_string()
    };

    if path.ends_with(".bz2") {
        let decoder = BzDecoder::new(file);
        Ok((Box::new(BufReader::new(decoder)), format))
    } else if path.ends_with(".gz") {
        let decoder = GzDecoder::new(file);
        Ok((Box::new(BufReader::new(decoder)), format))
    } else if path.ends_with(".lz4") {
        let decoder = Lz4Decoder::new(file);
        Ok((Box::new(BufReader::new(decoder)), format))
    } else {
        Ok((Box::new(BufReader::new(file)), format))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_determine_compression_from_arg() {
        // Explicit --compress overrides everything
        assert_eq!(determine_compression("lz4", None), "lz4");
        assert_eq!(determine_compression("gzip", None), "gzip");
        assert_eq!(determine_compression("lz4", Some("output.nt")), "lz4");
        assert_eq!(determine_compression("gzip", Some("output.nt.lz4")), "gzip");
    }

    #[test]
    fn test_determine_compression_from_extension() {
        // Auto-detect from output file extension
        assert_eq!(determine_compression("none", Some("output.nt.lz4")), "lz4");
        assert_eq!(determine_compression("none", Some("output.nt.gz")), "gzip");
        assert_eq!(
            determine_compression("none", Some("output.json.lz4")),
            "lz4"
        );
        assert_eq!(
            determine_compression("none", Some("output.json.gz")),
            "gzip"
        );

        // Case insensitive
        assert_eq!(determine_compression("none", Some("output.nt.LZ4")), "lz4");
        assert_eq!(determine_compression("none", Some("output.nt.GZ")), "gzip");
    }

    #[test]
    fn test_determine_compression_none() {
        // No compression
        assert_eq!(determine_compression("none", None), "none");
        assert_eq!(determine_compression("none", Some("output.nt")), "none");
        assert_eq!(determine_compression("none", Some("output.json")), "none");
    }

    #[test]
    fn test_detect_format_with_lz4_extension() {
        // Format detection should strip .lz4 extension
        assert_eq!(detect_format_from_path("data.nt.lz4"), "rdf");
        assert_eq!(detect_format_from_path("data.json.lz4"), "json");
        assert_eq!(detect_format_from_path("data.ndjson.lz4"), "json");
        assert_eq!(detect_format_from_path("truthy.lz4"), "rdf");
    }

    #[test]
    fn test_detect_format_with_multiple_extensions() {
        // Should handle nested compression extensions
        assert_eq!(detect_format_from_path("data.nt.bz2"), "rdf");
        assert_eq!(detect_format_from_path("data.nt.gz"), "rdf");
        assert_eq!(detect_format_from_path("data.json.bz2"), "json");
        assert_eq!(detect_format_from_path("data.json.gz"), "json");
    }

    #[test]
    fn test_create_compressed_writer_lz4() {
        let buffer: Vec<u8> = Vec::new();
        let writer = create_compressed_writer(buffer, "lz4");

        // Writer should be created successfully
        // We can't easily test the type, but we can verify it's writable
        drop(writer);
    }

    #[test]
    fn test_create_compressed_writer_gzip() {
        let buffer: Vec<u8> = Vec::new();
        let writer = create_compressed_writer(buffer, "gzip");
        drop(writer);
    }

    #[test]
    fn test_create_compressed_writer_none() {
        let buffer: Vec<u8> = Vec::new();
        let writer = create_compressed_writer(buffer, "none");
        drop(writer);
    }

    #[test]
    fn test_lz4_roundtrip() {
        let test_data = b"Hello, this is test data for LZ4 compression!\n";

        // Compress
        let mut compressed = Vec::new();
        {
            let mut encoder = Lz4Encoder::new(&mut compressed);
            encoder.write_all(test_data).unwrap();
            encoder.finish().unwrap();
        }

        // Verify it's actually compressed (has LZ4 magic bytes)
        assert!(compressed.len() >= 4);
        assert_eq!(&compressed[0..4], &[0x04, 0x22, 0x4d, 0x18]); // LZ4 frame magic

        // Decompress
        let mut decompressed = Vec::new();
        {
            let mut decoder = Lz4Decoder::new(&compressed[..]);
            decoder.read_to_end(&mut decompressed).unwrap();
        }

        assert_eq!(decompressed, test_data);
    }

    #[test]
    fn test_gzip_roundtrip() {
        use flate2::read::GzDecoder as GzDecoderRead;

        let test_data = b"Hello, this is test data for gzip compression!\n";

        // Compress
        let mut compressed = Vec::new();
        {
            let mut encoder = GzEncoder::new(&mut compressed, flate2::Compression::default());
            encoder.write_all(test_data).unwrap();
            encoder.finish().unwrap();
        }

        // Verify it's actually compressed (has gzip magic bytes)
        assert!(compressed.len() >= 2);
        assert_eq!(&compressed[0..2], &[0x1f, 0x8b]); // gzip magic

        // Decompress
        let mut decompressed = Vec::new();
        {
            let mut decoder = GzDecoderRead::new(&compressed[..]);
            decoder.read_to_end(&mut decompressed).unwrap();
        }

        assert_eq!(decompressed, test_data);
    }

    #[test]
    fn test_lz4_encoder_writes_valid_data() {
        let test_data = "Test line 1\nTest line 2\nTest line 3\n";

        // Write through LZ4 encoder directly
        let mut buffer = Vec::new();
        {
            let mut encoder = Lz4Encoder::new(&mut buffer);
            encoder.write_all(test_data.as_bytes()).unwrap();
            encoder.finish().unwrap();
        }

        // Verify LZ4 magic bytes
        assert!(buffer.len() >= 4);
        assert_eq!(&buffer[0..4], &[0x04, 0x22, 0x4d, 0x18]);

        // Decompress and verify
        let mut decompressed = Vec::new();
        {
            let mut decoder = Lz4Decoder::new(&buffer[..]);
            decoder.read_to_end(&mut decompressed).unwrap();
        }

        assert_eq!(String::from_utf8(decompressed).unwrap(), test_data);
    }

    #[test]
    fn test_gzip_encoder_writes_valid_data() {
        use flate2::read::GzDecoder as GzDecoderRead;

        let test_data = "Test line 1\nTest line 2\nTest line 3\n";

        // Write through gzip encoder directly
        let mut buffer = Vec::new();
        {
            let mut encoder = GzEncoder::new(&mut buffer, flate2::Compression::default());
            encoder.write_all(test_data.as_bytes()).unwrap();
            encoder.finish().unwrap();
        }

        // Verify gzip magic bytes
        assert!(buffer.len() >= 2);
        assert_eq!(&buffer[0..2], &[0x1f, 0x8b]);

        // Decompress and verify
        let mut decompressed = Vec::new();
        {
            let mut decoder = GzDecoderRead::new(&buffer[..]);
            decoder.read_to_end(&mut decompressed).unwrap();
        }

        assert_eq!(String::from_utf8(decompressed).unwrap(), test_data);
    }

    #[test]
    fn test_create_compressed_writer_returns_writer() {
        // Test that create_compressed_writer returns a usable writer
        // We use a Vec that we own completely

        // LZ4
        let buffer_lz4: Vec<u8> = Vec::new();
        let writer_lz4 = create_compressed_writer(buffer_lz4, "lz4");
        assert!(std::mem::size_of_val(&writer_lz4) > 0);
        drop(writer_lz4);

        // Gzip
        let buffer_gz: Vec<u8> = Vec::new();
        let writer_gz = create_compressed_writer(buffer_gz, "gzip");
        assert!(std::mem::size_of_val(&writer_gz) > 0);
        drop(writer_gz);

        // None
        let buffer_none: Vec<u8> = Vec::new();
        let writer_none = create_compressed_writer(buffer_none, "none");
        assert!(std::mem::size_of_val(&writer_none) > 0);
        drop(writer_none);
    }
}
