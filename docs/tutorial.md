# Wikidata Werkzeug Tutorial

A comprehensive guide to filtering and transforming Wikidata dumps with `wikidata-werkzeug`.

## Table of Contents

1. [Introduction](#introduction)
2. [Installation](#installation)
3. [Understanding Wikidata Dumps](#understanding-wikidata-dumps)
4. [Basic Usage](#basic-usage)
5. [Filtering by Claims](#filtering-by-claims)
6. [Language Filtering](#language-filtering)
7. [Format Conversion](#format-conversion)
8. [Compression](#compression)
9. [Performance Optimization](#performance-optimization)
10. [Real-World Examples](#real-world-examples)
11. [Troubleshooting](#troubleshooting)

---

## Introduction

`wikidata-werkzeug` is a fast, parallel Wikidata dump filter written in Rust. It allows you to:

- **Filter** entities by claims, properties, languages, and entity types
- **Convert** between RDF N-Triples and JSON formats
- **Compress** output with gzip or LZ4
- **Process** massive dumps efficiently using parallel processing

### Why Use This Tool?

Wikidata dumps are enormous (80+ GB compressed). Downloading and processing the full dump is impractical for most use cases. This tool lets you:

- Extract only the entities you need (e.g., all humans, all cities in Germany)
- Reduce file sizes by 90-99% by keeping only relevant data
- Convert between formats for different downstream applications
- Filter languages to keep only what you need

---

## Installation

### Prerequisites

- Rust toolchain (1.70 or later)
- ~2GB RAM for processing (more for larger batches)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/your-repo/wikidata-werkzeug.git
cd wikidata-werkzeug

# Build optimized release binary
cargo build --release

# The binary is at ./target/release/wikidata-werkzeug
```

### Verify Installation

```bash
./target/release/wikidata-werkzeug --help
```

---

## Understanding Wikidata Dumps

### Dump Types

Wikidata provides several dump formats:

| Dump | Format | Size | Contents |
|------|--------|------|----------|
| `latest-all.json.bz2` | JSON | ~80GB | Complete data with all statement details |
| `latest-truthy.nt.bz2` | N-Triples | ~80GB | Only "best" values, no qualifiers/references |
| `latest-lexemes.json.bz2` | JSON | ~2GB | Lexicographical data only |

### Where to Download

```bash
# Truthy N-Triples dump (recommended for most filtering tasks)
wget https://dumps.wikimedia.org/wikidatawiki/entities/latest-truthy.nt.bz2

# Full JSON dump (when you need complete statement data)
wget https://dumps.wikimedia.org/wikidatawiki/entities/latest-all.json.bz2
```

### Entity Structure

Every Wikidata entity has:

- **ID**: `Q42` (items) or `P31` (properties)
- **Labels**: Names in different languages
- **Descriptions**: Short descriptions in different languages
- **Aliases**: Alternative names
- **Claims**: Property-value statements (e.g., P31:Q5 = "instance of human")
- **Sitelinks**: Links to Wikipedia articles (JSON only)

---

## Basic Usage

### Command Structure

```bash
wikidata-werkzeug [OPTIONS] [INPUT]
```

- `INPUT`: File path or stdin if not provided
- Output goes to stdout by default (use `--output` for file)

### Your First Filter

Extract all entities that are instances of "human" (Q5):

```bash
wikidata-werkzeug --claim 'P31:Q5' input.nt.bz2 > humans.nt
```

### Reading from Stdin

```bash
bzcat input.nt.bz2 | wikidata-werkzeug --claim 'P31:Q5' > humans.nt
```

### Progress Reporting

Add `--progress` to see processing statistics:

```bash
wikidata-werkzeug --claim 'P31:Q5' --progress input.nt.bz2 > humans.nt
```

Output on stderr:
```
Processed 1000000 lines, 50000 entities, 25000 matched...
```

---

## Filtering by Claims

Claims are the most powerful filtering mechanism. They let you select entities based on their properties and values.

### Claim Syntax Overview

| Syntax | Meaning | Example |
|--------|---------|---------|
| `P31:Q5` | Property has specific value | Instance of human |
| `P31:Q5,Q6256` | Property has any of these values | Human OR country |
| `P31` | Property exists (any value) | Has "instance of" |
| `P31:Q5&P18` | Both conditions (AND) | Human with image |
| `P31:Q5\|P27:Q183` | Either condition (OR) | Human OR German citizen |
| `~P576` | Property does NOT exist | Not dissolved |

### Basic Claim Filter

```bash
# All humans
wikidata-werkzeug --claim 'P31:Q5' input.nt.bz2

# All countries
wikidata-werkzeug --claim 'P31:Q6256' input.nt.bz2

# All cities
wikidata-werkzeug --claim 'P31:Q515' input.nt.bz2
```

### Multiple Values (OR within property)

Use comma to match any of several values:

```bash
# Humans OR fictional humans
wikidata-werkzeug --claim 'P31:Q5,Q15632617' input.nt.bz2

# Cities, towns, or villages
wikidata-werkzeug --claim 'P31:Q515,Q3957,Q532' input.nt.bz2
```

### Property Existence

Check if a property exists without specifying a value:

```bash
# Entities that have an image (P18)
wikidata-werkzeug --claim 'P18' input.nt.bz2

# Entities that have coordinates (P625)
wikidata-werkzeug --claim 'P625' input.nt.bz2
```

### AND Conditions

Use `&` to require multiple conditions:

```bash
# German citizens who are politicians
wikidata-werkzeug --claim 'P27:Q183&P106:Q82955' input.nt.bz2

# Cities with population data and coordinates
wikidata-werkzeug --claim 'P31:Q515&P1082&P625' input.nt.bz2

# Humans with images and birth dates
wikidata-werkzeug --claim 'P31:Q5&P18&P569' input.nt.bz2
```

### OR Conditions

Use `|` to match any of several conditions:

```bash
# Humans OR countries
wikidata-werkzeug --claim 'P31:Q5|P31:Q6256' input.nt.bz2

# German OR Austrian citizens
wikidata-werkzeug --claim 'P27:Q183|P27:Q40' input.nt.bz2
```

### NOT Conditions

Use `~` to exclude entities:

```bash
# Cities that are NOT dissolved
wikidata-werkzeug --claim 'P31:Q515&~P576' input.nt.bz2

# Living people (no death date)
wikidata-werkzeug --claim 'P31:Q5&~P570' input.nt.bz2

# Companies that are still active
wikidata-werkzeug --claim 'P31:Q4830453&~P576' input.nt.bz2
```

### Complex Expressions

Combine operators for sophisticated filters:

```bash
# German politicians who are still alive
wikidata-werkzeug --claim 'P27:Q183&P106:Q82955&~P570' input.nt.bz2

# Cities or municipalities in Germany
wikidata-werkzeug --claim 'P31:Q515,Q262166&P17:Q183' input.nt.bz2

# Scientists with Nobel Prize
wikidata-werkzeug --claim 'P106:Q901&P166:Q7191' input.nt.bz2
```

### Operator Precedence

`|` (OR) has **lower** precedence than `&` (AND):

```
A & B | C  =  (A AND B) OR C
A | B & C  =  A OR (B AND C)
```

To combine differently, structure your query accordingly or run multiple passes.

---

## Language Filtering

Reduce output size dramatically by keeping only the languages you need.

### Basic Language Filter

```bash
# Keep only German and English
wikidata-werkzeug --languages de,en input.nt.bz2 > output.nt

# Keep only English
wikidata-werkzeug --languages en input.nt.bz2 > output.nt
```

### Language Subvariants

By default, language codes include their subvariants:

- `de` includes `de`, `de-ch`, `de-at`, `de-formal`
- `en` includes `en`, `en-gb`, `en-us`, `en-ca`
- `zh` includes `zh`, `zh-hans`, `zh-hant`, `zh-cn`, `zh-tw`

### Exact Language Matching

Disable subvariant matching:

```bash
# Only "de", not "de-ch" or "de-at"
wikidata-werkzeug --languages de --language-exact-match input.nt.bz2

# Only standard English, not British or American variants
wikidata-werkzeug --languages en --language-exact-match input.nt.bz2
```

### Combining with Claim Filters

```bash
# German cities with German and English labels only
wikidata-werkzeug \
  --claim 'P31:Q515&P17:Q183' \
  --languages de,en \
  input.nt.bz2 > german-cities.nt
```

---

## Format Conversion

Convert between RDF N-Triples and JSON formats.

### Output Format Options

| Value | Description |
|-------|-------------|
| `same` | Keep input format (default) |
| `ntriples` | Output as N-Triples |
| `json` | Output as NDJSON (one JSON object per line) |

### N-Triples to JSON

```bash
# Convert RDF to JSON
wikidata-werkzeug \
  --output-format json \
  --languages de,en \
  input.nt.bz2 > entities.ndjson
```

Output format:
```json
{"id":"Q183","type":"item","labels":{"de":{"language":"de","value":"Deutschland"},"en":{"language":"en","value":"Germany"}},"descriptions":{...},"claims":{...}}
```

### JSON to N-Triples

```bash
# Convert JSON to RDF
wikidata-werkzeug \
  --output-format ntriples \
  input.json.bz2 > entities.nt
```

### Filter and Convert

```bash
# Filter humans and convert to JSON
wikidata-werkzeug \
  --claim 'P31:Q5' \
  --output-format json \
  --languages en \
  input.nt.bz2 > humans.ndjson
```

### JSON Attribute Filtering

Reduce JSON output by keeping/omitting specific attributes:

```bash
# Keep only id, labels, and descriptions
wikidata-werkzeug \
  --keep id,type,labels,descriptions \
  input.json.bz2 > minimal.ndjson

# Remove claims and sitelinks (keep everything else)
wikidata-werkzeug \
  --omit claims,sitelinks \
  input.json.bz2 > smaller.ndjson
```

Valid attributes: `id`, `type`, `labels`, `descriptions`, `aliases`, `claims`, `sitelinks`

---

## Compression

### Input Compression (Automatic)

The tool automatically decompresses:

| Extension | Format |
|-----------|--------|
| `.bz2` | bzip2 |
| `.gz` | gzip |
| `.lz4` | LZ4 frame |

```bash
# All these work automatically
wikidata-werkzeug input.nt.bz2
wikidata-werkzeug input.nt.gz
wikidata-werkzeug input.nt.lz4
```

### Output Compression

#### Via File Extension

```bash
# Auto-detect from output filename
wikidata-werkzeug --output filtered.nt.gz input.nt.bz2
wikidata-werkzeug --output filtered.nt.lz4 input.nt.bz2
```

#### Via --compress Flag

```bash
# Explicit compression to stdout
wikidata-werkzeug --compress gzip input.nt.bz2 > filtered.nt.gz
wikidata-werkzeug --compress lz4 input.nt.bz2 > filtered.nt.lz4
```

### Compression Comparison

| Format | Speed | Ratio | Best For |
|--------|-------|-------|----------|
| None | Fastest | 1x | Piping to other tools |
| LZ4 | Very fast | ~3x | Large files, fast decompression |
| Gzip | Moderate | ~5x | Archival, compatibility |

### Combined Example

```bash
# Filter, convert, and compress
wikidata-werkzeug \
  --claim 'P31:Q515' \
  --languages de,en \
  --output-format json \
  --output cities.json.lz4 \
  latest-truthy.nt.bz2
```

---

## Performance Optimization

### Thread Configuration

By default, uses all CPU cores. Adjust if needed:

```bash
# Use 8 threads
wikidata-werkzeug --threads 8 input.nt.bz2 > output.nt

# Use 4 threads (lower memory usage)
wikidata-werkzeug --threads 4 input.nt.bz2 > output.nt
```

### Batch Size

Adjust batch size for memory/speed tradeoff:

```bash
# Larger batches (more memory, potentially faster)
wikidata-werkzeug --batch-size 500 input.nt.bz2

# Smaller batches (less memory)
wikidata-werkzeug --batch-size 50 input.nt.bz2
```

Default batch sizes:
- RDF: 100 entities per batch
- JSON: 1000 entities per batch

### Resuming Interrupted Jobs

If processing is interrupted, resume with `--skip-lines`:

```bash
# Skip first 10 million lines (resume from there)
wikidata-werkzeug --skip-lines 10000000 input.nt.bz2 >> output.nt
```

### Processing Subsets

Test your filters on a subset first:

```bash
# Process only first 1 million lines
wikidata-werkzeug --max-lines 1000000 --claim 'P31:Q5' input.nt.bz2 > test.nt
```

### Estimated Processing Times

For a full Wikidata truthy dump (~15 billion triples):

| Operation | Time (16 cores) | Time (4 cores) |
|-----------|-----------------|----------------|
| Simple claim filter | 2-4 hours | 6-12 hours |
| Complex claim filter | 3-5 hours | 8-16 hours |
| With language filter | +10-20% | +10-20% |
| With format conversion | +20-30% | +20-30% |

---

## Real-World Examples

### Example 1: German Knowledge Graph

Extract all entities relevant to German administration:

```bash
#!/bin/bash
INPUT="latest-truthy.nt.bz2"
OUTPUT_DIR="./german-kg"
mkdir -p "$OUTPUT_DIR"

# 1. German states, districts, cities, municipalities
wikidata-werkzeug \
  --claim 'P31:Q1221156,Q106658,Q22865,Q262166,Q515' \
  --languages de,en \
  --progress \
  "$INPUT" > "$OUTPUT_DIR/admin-regions.nt"

# 2. German politicians
wikidata-werkzeug \
  --claim 'P27:Q183&P106:Q82955' \
  --languages de,en \
  --progress \
  "$INPUT" > "$OUTPUT_DIR/politicians.nt"

# 3. German universities
wikidata-werkzeug \
  --claim 'P31:Q3918&P17:Q183' \
  --languages de,en \
  --progress \
  "$INPUT" > "$OUTPUT_DIR/universities.nt"

# Combine and deduplicate
cat "$OUTPUT_DIR"/*.nt | sort -u > "$OUTPUT_DIR/complete.nt"
```

### Example 2: Scientists Database

Create a JSON database of scientists:

```bash
wikidata-werkzeug \
  --claim 'P106:Q901' \
  --output-format json \
  --keep id,type,labels,descriptions,claims \
  --languages en,de,fr,es,zh \
  --output scientists.json.lz4 \
  --progress \
  latest-truthy.nt.bz2
```

### Example 3: Geographic Entities with Coordinates

Extract places that have coordinates:

```bash
wikidata-werkzeug \
  --claim 'P625' \
  --property P31,P625,P17,P1082 \
  --languages en \
  --output places-with-coords.nt.gz \
  --progress \
  latest-truthy.nt.bz2
```

### Example 4: Living People for Wikipedia Bot

```bash
wikidata-werkzeug \
  --claim 'P31:Q5&P569&~P570' \
  --output-format json \
  --keep id,labels,descriptions,claims \
  --languages en \
  --output living-people.ndjson.lz4 \
  --progress \
  latest-all.json.bz2
```

### Example 5: Movies with Directors and Release Dates

```bash
wikidata-werkzeug \
  --claim 'P31:Q11424&P57&P577' \
  --output-format json \
  --languages en,de \
  --output movies.json.gz \
  --progress \
  latest-truthy.nt.bz2
```

---

## Troubleshooting

### Common Issues

#### "No output" or empty results

1. **Check your claim syntax**: Use single quotes around claim expressions
   ```bash
   # Correct
   wikidata-werkzeug --claim 'P31:Q5' input.nt
   
   # Wrong (shell interprets special characters)
   wikidata-werkzeug --claim P31:Q5&P18 input.nt
   ```

2. **Verify entity IDs**: Make sure Q/P IDs are correct
   - Check on [wikidata.org](https://www.wikidata.org)

3. **Test with --max-lines first**:
   ```bash
   wikidata-werkzeug --claim 'P31:Q5' --max-lines 100000 input.nt
   ```

#### Memory issues

Reduce batch size and thread count:
```bash
wikidata-werkzeug --threads 2 --batch-size 25 input.nt
```

#### Slow processing

1. Use release build (not debug):
   ```bash
   cargo build --release
   ./target/release/wikidata-werkzeug ...
   ```

2. Increase batch size if you have memory:
   ```bash
   wikidata-werkzeug --batch-size 500 input.nt
   ```

3. Use LZ4 output instead of gzip:
   ```bash
   wikidata-werkzeug --compress lz4 input.nt > output.nt.lz4
   ```

#### Format detection issues

Explicitly specify format:
```bash
wikidata-werkzeug --format rdf input.nt
wikidata-werkzeug --format json input.json
```

### Getting Help

```bash
# Full help
wikidata-werkzeug --help

# Version
wikidata-werkzeug --version
```

---

## Quick Reference

### Essential Commands

```bash
# Filter by claim
wikidata-werkzeug --claim 'P31:Q5' input.nt.bz2 > output.nt

# Filter with multiple conditions
wikidata-werkzeug --claim 'P31:Q5&P27:Q183' input.nt.bz2 > output.nt

# Filter languages
wikidata-werkzeug --languages de,en input.nt.bz2 > output.nt

# Convert format
wikidata-werkzeug --output-format json input.nt.bz2 > output.json

# Compress output
wikidata-werkzeug --output output.nt.lz4 input.nt.bz2

# Show progress
wikidata-werkzeug --progress input.nt.bz2 > output.nt
```

### Common Wikidata IDs

| ID | Meaning |
|----|---------|
| Q5 | Human |
| Q6256 | Country |
| Q515 | City |
| Q7278 | Political party |
| Q11424 | Film |
| Q5398426 | Television series |
| Q7889 | Video game |
| Q571 | Book |
| P31 | Instance of |
| P279 | Subclass of |
| P17 | Country |
| P27 | Country of citizenship |
| P106 | Occupation |
| P569 | Date of birth |
| P570 | Date of death |
| P576 | Dissolved/abolished date |
| P625 | Coordinate location |

---

## Next Steps

- Explore the [README](../README.md) for complete option reference
- Check [queries/german-administration.md](../queries/german-administration.md) for German-specific examples
- Run the extraction script [queries/extract-german-admin.sh](../queries/extract-german-admin.sh)

Happy filtering!
