# wikidata-werkzeug

A fast, parallel Wikidata dump filter written in Rust. Supports both RDF (N-Triples) and JSON formats.

## Installation

```bash
cargo build --release
```

## Usage

```bash
wikidata-werkzeug [OPTIONS] [INPUT]
```

### Basic Examples

```bash
# Filter entities that are instances of human (Q5)
wikidata-werkzeug --claim 'P31:Q5' input.nt.bz2 > humans.nt

# Filter with multiple languages
wikidata-werkzeug --claim 'P31:Q6256' --languages de,en input.nt.bz2 > countries.nt

# Show progress
wikidata-werkzeug --claim 'P31:Q515' --progress input.nt.bz2 > cities.nt
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--claim <CLAIM>` | `-c` | Filter by claim expression (see Claim Syntax below) |
| `--languages <LANGS>` | `-l` | Filter languages (comma-separated, e.g., `de,en,fr`) |
| `--language-exact-match` | | Disable subvariant matching (e.g., `de` won't include `de-ch`) |
| `--type <TYPE>` | `-t` | Entity type: `item`, `property`, or `both` (default: `item`) |
| `--format <FORMAT>` | `-f` | Input format: `auto`, `rdf`, `json` (default: `auto`) |
| `--output-format <FORMAT>` | `-o` | Output format: `same`, `ntriples`, `json` (default: `same`) |
| `--output <FILE>` | | Output file (stdout if not provided, compression auto-detected) |
| `--compress <TYPE>` | | Output compression: `none`, `gzip`, `lz4` (default: `none`) |
| `--subject <IDS>` | | Keep only specified entity IDs (comma-separated) |
| `--property <IDS>` | | Keep only specified properties (comma-separated) |
| `--keep <ATTRS>` | | Keep only specified entity attributes (JSON only) |
| `--omit <ATTRS>` | | Omit specified entity attributes (JSON only) |
| `--progress` | `-p` | Show progress on stderr |
| `--threads <N>` | | Number of threads (default: number of CPUs) |
| `--batch-size <N>` | | Batch size for parallel processing |
| `--skip-lines <N>` | | Skip first N lines (useful for resuming) |
| `--max-lines <N>` | | Stop after N lines (0 = no limit) |

## Claim Syntax

The `--claim` option supports a flexible expression syntax:

### Basic Expressions

| Expression | Description |
|------------|-------------|
| `P31:Q5` | Property P31 has value Q5 |
| `P31:Q5,Q6256` | Property P31 has value Q5 OR Q6256 |
| `P18` | Property P18 exists (has any value) |

### Logical Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `&` | AND | `P31:Q5&P18` (is human AND has image) |
| `\|` | OR | `P31:Q5\|P31:Q6256` (is human OR is country) |
| `~` | NOT | `~P576` (not dissolved) |

### Complex Expressions

```bash
# German politicians: German citizen AND occupation is politician
--claim 'P27:Q183&P106:Q82955'

# Cities or countries
--claim 'P31:Q515|P31:Q6256'

# Active entities (not dissolved)
--claim 'P31:Q515&~P576'
```

### Precedence

`|` (OR) has lower precedence than `&` (AND), so `A&B|C` means `(A AND B) OR C`.

## Filter Attributes (JSON only)

Wikidata entities have the following attributes: `id`, `type`, `labels`, `descriptions`, `aliases`, `claims`, `sitelinks`.

These attributes can take a lot of space. If you don't need all of them, you can filter them with `--keep` or `--omit`:

```bash
# Keep only id, labels, and descriptions (omit claims and sitelinks)
cat entities.json | wikidata-werkzeug --omit claims,sitelinks > smaller.ndjson

# Equivalent using --keep
cat entities.json | wikidata-werkzeug --keep id,type,labels,descriptions,aliases > smaller.ndjson
```

**Valid attributes:** `id`, `type`, `labels`, `descriptions`, `aliases`, `claims`, `sitelinks`

**Note:** `--keep` and `--omit` cannot be used together.

## Language Filter

The `--languages` option filters all triples with language tags:

```bash
# Keep only German and English labels/descriptions
wikidata-werkzeug --languages de,en input.nt > output.nt
```

### Language Subvariants

By default, language subvariants are included:
- `--languages de` keeps `de`, `de-ch`, `de-at`, etc.
- `--languages en` keeps `en`, `en-gb`, `en-us`, etc.

To disable subvariant matching:

```bash
# Only exact "de", not "de-ch" or "de-at"
wikidata-werkzeug --languages de --language-exact-match input.nt > output.nt
```

## Supported Formats

### Input

- **RDF N-Triples** (`.nt`, `.nt.bz2`, `.nt.gz`)
- **JSON/NDJSON** (`.json`, `.ndjson`, `.json.bz2`, `.json.gz`)

Format is auto-detected from file extension or can be specified with `--format`.

### Output

By default, the output format matches the input format. Use `--output-format` to convert:

| Value | Description |
|-------|-------------|
| `same` | Keep same format as input (default) |
| `ntriples` | Output as N-Triples |
| `json` | Output as NDJSON (one JSON object per line) |

### Compression

**Input** - Automatically decompresses:
- **bzip2** (`.bz2`)
- **gzip** (`.gz`)
- **LZ4** (`.lz4`)

**Output** - Compression auto-detected from `--output` extension or via `--compress`:
- **gzip** (`.gz` or `--compress gzip`)
- **LZ4** (`.lz4` or `--compress lz4`)

```bash
# Auto-detect compression from output filename
wikidata-werkzeug --output filtered.nt.lz4 input.nt.bz2

# Explicit compression to stdout
wikidata-werkzeug --compress lz4 input.nt.bz2 > filtered.nt.lz4

# Combine with format conversion
wikidata-werkzeug --output-format json --output entities.json.lz4 input.nt.bz2
```

## Performance

- Parallel processing with configurable thread count
- Batch processing for optimal throughput
- Large output buffer (8 MB) for efficient I/O
- Supports resuming interrupted jobs with `--skip-lines`

## Examples

### Extract German Administrative Entities

```bash
# Federal states, districts, municipalities
wikidata-werkzeug \
  --claim 'P31:Q1221156,Q106658,Q22865,Q262166,Q515' \
  --languages de,en \
  --progress \
  latest-truthy.nt.bz2 > german-admin.nt
```

### Extract German Politicians

```bash
wikidata-werkzeug \
  --claim 'P27:Q183&P106:Q82955' \
  --languages de,en \
  --progress \
  latest-truthy.nt.bz2 > german-politicians.nt
```

### Filter Specific Properties

```bash
# Keep only P31 (instance of) and P279 (subclass of) triples
wikidata-werkzeug \
  --property P31,P279 \
  input.nt.bz2 > taxonomy.nt
```

### Reduce JSON Output Size

```bash
# For full-text search: keep only labels, aliases, and descriptions
wikidata-werkzeug \
  --claim 'P31:Q5' \
  --omit claims,sitelinks \
  --languages de,en \
  latest-all.json.bz2 > humans-minimal.ndjson

# Equivalent using --keep
wikidata-werkzeug \
  --claim 'P31:Q5' \
  --keep id,type,labels,descriptions,aliases \
  --languages de,en \
  latest-all.json.bz2 > humans-minimal.ndjson
```

### Convert N-Triples to JSON

Convert RDF N-Triples to Wikidata-compatible JSON format:

```bash
# Convert all entities to JSON
wikidata-werkzeug \
  --output-format json \
  --languages de,en \
  latest-truthy.nt.bz2 > entities.ndjson

# Filter and convert in one step
wikidata-werkzeug \
  --claim 'P31:Q6256' \
  --output-format json \
  --languages de,en \
  latest-truthy.nt.bz2 > countries.ndjson
```

The JSON output follows the Wikidata entity format:

```json
{
  "id": "Q183",
  "type": "item",
  "labels": {
    "de": {"language": "de", "value": "Deutschland"},
    "en": {"language": "en", "value": "Germany"}
  },
  "descriptions": {
    "de": {"language": "de", "value": "Staat in Mitteleuropa"},
    "en": {"language": "en", "value": "country in Central Europe"}
  },
  "aliases": {
    "de": [
      {"language": "de", "value": "Bundesrepublik Deutschland"},
      {"language": "de", "value": "BRD"}
    ]
  },
  "claims": {
    "P31": [{
      "mainsnak": {
        "snaktype": "value",
        "property": "P31",
        "datavalue": {
          "type": "wikibase-entityid",
          "value": {"entity-type": "item", "id": "Q6256"}
        }
      },
      "type": "statement",
      "rank": "normal"
    }]
  }
}
```

**Note:** When converting from N-Triples, only entity-valued claims (references to Q/P items) are included in the JSON output. Literal values (strings, numbers, dates) from N-Triples are not converted to claims, but labels, descriptions, and aliases are extracted from their respective RDF predicates (`rdfs:label`, `schema:description`, `skos:altLabel`).

## License

MIT
