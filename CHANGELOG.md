# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Code refactoring**: Extracted main.rs (~1800 lines) into focused modules for better maintainability
  - `rdf.rs`: RdfEntity, RdfRegexes, RDF processing (~910 lines)
  - `json.rs`: JSON processing, JSON-to-NTriples conversion (~420 lines)
  - `compression.rs`: Compression/decompression, reader/writer creation (~300 lines)
  - `main.rs`: Now only CLI entry point and argument parsing (~270 lines)
- Updated LLM.txt with new project structure

### Added

- **RDF to JSON conversion**: The `--output-format=json` option now works for N-Triples input, converting RDF data to Wikidata-compatible JSON format (NDJSON)
- Labels extraction from `rdfs:label` triples
- Descriptions extraction from `schema:description` triples
- Aliases extraction from `skos:altLabel` triples
- Entity-valued claims are converted to Wikidata JSON claim format with `mainsnak` structure
- Language filtering is applied to labels, descriptions, and aliases during conversion
- New tests for RDF-to-JSON conversion functionality (15 new test cases)
- **LZ4 compression support**: Input and output now support LZ4 frame format (via `lz4_flex`)
  - Input: Automatically decompresses `.lz4` files
  - Output: Use `--output file.lz4` or `--compress lz4`
- **Output file option**: New `--output` option to write directly to a file instead of stdout
- **Gzip output compression**: Use `--output file.gz` or `--compress gzip`
- Compression is auto-detected from output file extension
- **JSON to N-Triples conversion**: The `--output-format=ntriples` option now works for JSON input
  - Converts labels, descriptions, aliases to RDF triples
  - Converts claims with various datatypes (entity references, strings, quantities, times, coordinates, monolingualtext)
  - 5 new tests for JSON-to-NTriples conversion

### Fixed

- `--output-format` option was defined but not implemented - now fully functional for RDF input
- `--output-format ntriples` was ignored for JSON input, always outputting JSON - now correctly converts to N-Triples

### Notes

- When converting from N-Triples to JSON, only entity-valued claims (Q/P references) are included
- Literal values (strings, numbers, dates, coordinates) from N-Triples are not converted to claims
- The JSON output format is compatible with Wikidata's entity JSON structure

## [0.1.0] - 2026-01-21

### Added

- Initial release
- Filter Wikidata dumps by claims with flexible expression syntax (AND, OR, NOT)
- Support for RDF N-Triples and JSON/NDJSON input formats
- Automatic format detection from file extension
- Parallel processing with configurable thread count
- Language filtering with subvariant support
- Property filtering
- Subject (entity ID) filtering
- Entity type filtering (item/property)
- Attribute filtering for JSON (`--keep`/`--omit`)
- bzip2 and gzip compression support
- Progress reporting
- Skip/resume functionality with `--skip-lines` and `--max-lines`
