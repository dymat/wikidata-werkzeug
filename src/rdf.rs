use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rayon::prelude::*;
use regex::Regex;

use crate::filter::EntityFilter;
use crate::FilterError;

/// Output format for processing
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OutputFormat {
    NTriples,
    Json,
}

/// Represents a parsed RDF entity with all its data
#[derive(Clone)]
pub struct RdfEntity {
    pub id: String,
    pub metadata: Vec<String>,
    pub triples: Vec<String>,
    pub claims: HashMap<String, HashSet<String>>,
    pub entity_type: Option<String>,
    /// Labels by language code (e.g., "de" -> "Deutschland")
    pub labels: HashMap<String, String>,
    /// Descriptions by language code
    pub descriptions: HashMap<String, String>,
    /// Aliases by language code (multiple per language)
    pub aliases: HashMap<String, Vec<String>>,
}

/// Thread-safe regex container for RDF parsing
pub struct RdfRegexes {
    pub entity_re: Regex,
    pub entity_data_re: Regex,
    pub prop_direct_re: Regex,
    pub entity_value_re: Regex,
    pub type_re: Regex,
    /// Matches rdfs:label predicate
    pub label_re: Regex,
    /// Matches schema:description predicate
    pub description_re: Regex,
    /// Matches skos:altLabel predicate
    pub alias_re: Regex,
    /// Extracts language-tagged literal: "value"@lang
    pub lang_literal_re: Regex,
}

impl RdfRegexes {
    pub fn new() -> Self {
        Self {
            entity_re: Regex::new(r"^<http://www\.wikidata\.org/entity/(Q\d+)>").unwrap(),
            entity_data_re: Regex::new(
                r"^<https://www\.wikidata\.org/wiki/Special:EntityData/(Q\d+)>",
            )
            .unwrap(),
            prop_direct_re: Regex::new(
                r"<http://www\.wikidata\.org/prop/direct(?:-normalized)?/(P\d+)>",
            )
            .unwrap(),
            entity_value_re: Regex::new(r"<http://www\.wikidata\.org/entity/(Q\d+)>\s*\.$")
                .unwrap(),
            type_re: Regex::new(r"<http://wikiba\.se/ontology#(Item|Property)>").unwrap(),
            label_re: Regex::new(r"<http://www\.w3\.org/2000/01/rdf-schema#label>").unwrap(),
            description_re: Regex::new(r"<http://schema\.org/description>").unwrap(),
            alias_re: Regex::new(r"<http://www\.w3\.org/2004/02/skos/core#altLabel>").unwrap(),
            lang_literal_re: Regex::new(r#""(.*)"\s*@([a-zA-Z0-9-]+)\s*\.\s*$"#).unwrap(),
        }
    }
}

impl Default for RdfRegexes {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create RdfEntity and reset state
fn create_entity(
    id: &str,
    metadata: &mut Vec<String>,
    triples: &mut Vec<String>,
    claims: &mut HashMap<String, HashSet<String>>,
    entity_type: &mut Option<String>,
    labels: &mut HashMap<String, String>,
    descriptions: &mut HashMap<String, String>,
    aliases: &mut HashMap<String, Vec<String>>,
) -> RdfEntity {
    RdfEntity {
        id: id.to_string(),
        metadata: std::mem::take(metadata),
        triples: std::mem::take(triples),
        claims: std::mem::take(claims),
        entity_type: entity_type.take(),
        labels: std::mem::take(labels),
        descriptions: std::mem::take(descriptions),
        aliases: std::mem::take(aliases),
    }
}

/// Extract language tag from an RDF line
pub fn extract_language_tag(line: &str) -> Option<String> {
    // Look for language tag like "text"@en
    if let Some(at_pos) = line.rfind("@") {
        let after_at = &line[at_pos + 1..];
        // Find end of language tag (space or end of line)
        let end_pos = after_at
            .find(|c: char| c.is_whitespace() || c == '.')
            .unwrap_or(after_at.len());
        let lang = &after_at[..end_pos];
        if !lang.is_empty() && lang.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Some(lang.to_string());
        }
    }
    None
}

/// Process a batch of RDF entities in parallel
fn process_rdf_batch_parallel(batch: &[RdfEntity], filter: &Arc<EntityFilter>) -> Vec<RdfEntity> {
    batch
        .par_iter()
        .filter(|entity| filter.matches(&entity.id, &entity.claims, entity.entity_type.as_deref()))
        .cloned()
        .collect()
}

/// Write header lines efficiently
fn write_header_batch<W: Write>(output: &mut W, headers: &[String]) -> std::io::Result<u64> {
    if headers.is_empty() {
        return Ok(0);
    }

    let mut buffer = String::with_capacity(headers.len() * 200);
    for h in headers {
        buffer.push_str(h);
        buffer.push('\n');
    }
    output.write_all(buffer.as_bytes())?;
    Ok(headers.len() as u64)
}

/// Write RDF entities efficiently using batch writes
/// Returns (entities_written, triples_written)
fn write_rdf_entities_batch<W: Write>(
    output: &mut W,
    entities: &[RdfEntity],
) -> std::io::Result<(u64, u64)> {
    if entities.is_empty() {
        return Ok((0, 0));
    }

    // Pre-calculate total size for efficient allocation
    let total_lines: usize = entities
        .iter()
        .map(|e| e.metadata.len() + e.triples.len())
        .sum();

    // Estimate ~100 bytes per line average
    let mut buffer = String::with_capacity(total_lines * 100);

    let mut triples_count: u64 = 0;

    for entity in entities {
        for meta in &entity.metadata {
            buffer.push_str(meta);
            buffer.push('\n');
            triples_count += 1;
        }
        for triple in &entity.triples {
            buffer.push_str(triple);
            buffer.push('\n');
            triples_count += 1;
        }
    }

    // Single write call for entire batch
    output.write_all(buffer.as_bytes())?;

    Ok((entities.len() as u64, triples_count))
}

/// Convert RDF entity to Wikidata-compatible JSON format
pub fn rdf_entity_to_json(entity: &RdfEntity) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    // id
    obj.insert("id".to_string(), serde_json::json!(entity.id));

    // type
    let etype = entity.entity_type.as_deref().unwrap_or("item");
    obj.insert("type".to_string(), serde_json::json!(etype));

    // labels - Wikidata format: {"en": {"language": "en", "value": "Germany"}}
    if !entity.labels.is_empty() {
        let mut labels_obj = serde_json::Map::new();
        for (lang, value) in &entity.labels {
            labels_obj.insert(
                lang.clone(),
                serde_json::json!({
                    "language": lang,
                    "value": value
                }),
            );
        }
        obj.insert("labels".to_string(), serde_json::Value::Object(labels_obj));
    }

    // descriptions - same format as labels
    if !entity.descriptions.is_empty() {
        let mut desc_obj = serde_json::Map::new();
        for (lang, value) in &entity.descriptions {
            desc_obj.insert(
                lang.clone(),
                serde_json::json!({
                    "language": lang,
                    "value": value
                }),
            );
        }
        obj.insert(
            "descriptions".to_string(),
            serde_json::Value::Object(desc_obj),
        );
    }

    // aliases - Wikidata format: {"en": [{"language": "en", "value": "FRG"}]}
    if !entity.aliases.is_empty() {
        let mut aliases_obj = serde_json::Map::new();
        for (lang, values) in &entity.aliases {
            let alias_arr: Vec<serde_json::Value> = values
                .iter()
                .map(|v| {
                    serde_json::json!({
                        "language": lang,
                        "value": v
                    })
                })
                .collect();
            aliases_obj.insert(lang.clone(), serde_json::Value::Array(alias_arr));
        }
        obj.insert(
            "aliases".to_string(),
            serde_json::Value::Object(aliases_obj),
        );
    }

    // claims - Wikidata format with mainsnak structure
    // Only include claims that have at least one entity value (skip literal-only claims)
    if !entity.claims.is_empty() {
        let mut claims_obj = serde_json::Map::new();
        for (prop_id, values) in &entity.claims {
            // Skip claims with no entity values (these are literal values which we can't represent)
            if values.is_empty() {
                continue;
            }
            let statements: Vec<serde_json::Value> = values
                .iter()
                .map(|value_id| {
                    serde_json::json!({
                        "mainsnak": {
                            "snaktype": "value",
                            "property": prop_id,
                            "datavalue": {
                                "value": {
                                    "entity-type": if value_id.starts_with('P') { "property" } else { "item" },
                                    "id": value_id
                                },
                                "type": "wikibase-entityid"
                            }
                        },
                        "type": "statement",
                        "rank": "normal"
                    })
                })
                .collect();
            claims_obj.insert(prop_id.clone(), serde_json::Value::Array(statements));
        }
        if !claims_obj.is_empty() {
            obj.insert("claims".to_string(), serde_json::Value::Object(claims_obj));
        }
    }

    serde_json::Value::Object(obj)
}

/// Write RDF entities as JSON (NDJSON format)
fn write_rdf_entities_as_json_batch<W: Write>(
    output: &mut W,
    entities: &[RdfEntity],
) -> std::io::Result<(u64, u64)> {
    if entities.is_empty() {
        return Ok((0, 0));
    }

    let mut buffer = String::new();

    for entity in entities {
        let json = rdf_entity_to_json(entity);
        if let Ok(line) = serde_json::to_string(&json) {
            buffer.push_str(&line);
            buffer.push('\n');
        }
    }

    output.write_all(buffer.as_bytes())?;

    Ok((entities.len() as u64, entities.len() as u64))
}

/// Write RDF entities to output in the specified format
fn write_rdf_output_batch<W: Write>(
    output: &mut W,
    entities: &[RdfEntity],
    format: OutputFormat,
) -> std::io::Result<(u64, u64)> {
    match format {
        OutputFormat::NTriples => write_rdf_entities_batch(output, entities),
        OutputFormat::Json => write_rdf_entities_as_json_batch(output, entities),
    }
}

/// Main RDF filtering function with parallel processing
pub fn filter_rdf_parallel<R: BufRead, W: Write>(
    reader: R,
    output: &mut W,
    filter: &Arc<EntityFilter>,
    show_progress: bool,
    batch_size: usize,
    skip_lines: u64,
    max_lines: u64,
    output_format: OutputFormat,
) -> Result<(), FilterError> {
    let regexes = RdfRegexes::new();

    let mut current_entity: Option<String> = None;
    let mut current_triples: Vec<String> = Vec::new();
    let mut current_metadata: Vec<String> = Vec::new();
    let mut entity_claims: HashMap<String, HashSet<String>> = HashMap::new();
    let mut entity_type: Option<String> = None;
    let mut entity_labels: HashMap<String, String> = HashMap::new();
    let mut entity_descriptions: HashMap<String, String> = HashMap::new();
    let mut entity_aliases: HashMap<String, Vec<String>> = HashMap::new();

    let lines_processed = AtomicU64::new(0);
    let lines_skipped = AtomicU64::new(0);
    let entities_matched = AtomicU64::new(0);
    let triples_output = AtomicU64::new(0);
    let mut header_written = false;
    let mut skip_mode = skip_lines > 0;
    // After skipping, wait for next entity boundary to avoid partial entities
    let mut waiting_for_entity_boundary = skip_lines > 0;

    let mut header_lines: Vec<String> = Vec::new();
    let mut entity_batch: Vec<RdfEntity> = Vec::with_capacity(batch_size);

    let mut lines_actually_processed: u64 = 0;

    for line_result in reader.lines() {
        let line = line_result?;
        let current_line = lines_processed.fetch_add(1, Ordering::Relaxed) + 1;

        // Skip lines if needed
        if skip_mode {
            lines_skipped.fetch_add(1, Ordering::Relaxed);
            if current_line >= skip_lines {
                skip_mode = false;
                if show_progress {
                    eprintln!(
                        "Skipped {} lines, waiting for next entity boundary...",
                        skip_lines
                    );
                }
            }
            continue;
        }

        // After skipping, wait until we hit a new entity (EntityData line)
        if waiting_for_entity_boundary {
            lines_skipped.fetch_add(1, Ordering::Relaxed);
            if regexes.entity_data_re.is_match(&line) {
                waiting_for_entity_boundary = false;
                if show_progress {
                    eprintln!(
                        "Found entity boundary at line {}, starting processing...",
                        current_line
                    );
                }
                // Continue to process this line below
            } else {
                continue;
            }
        }

        // Count actually processed lines (after skip)
        lines_actually_processed += 1;

        // Check max_lines limit (counts lines after skip)
        if max_lines < u64::MAX && lines_actually_processed > max_lines {
            if show_progress {
                eprintln!("Reached max_lines limit ({}), stopping.", max_lines);
            }
            break;
        }

        if show_progress && lines_actually_processed % 100000 == 0 {
            eprintln!(
                "Line {} (skipped {}), processed {}, matched {} entities, output {} triples",
                current_line,
                lines_skipped.load(Ordering::Relaxed),
                lines_actually_processed,
                entities_matched.load(Ordering::Relaxed),
                triples_output.load(Ordering::Relaxed)
            );
        }

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.contains("wikiba.se/ontology#Dump") {
            header_lines.push(line);
            continue;
        }

        // Check for EntityData metadata line
        if let Some(caps) = regexes.entity_data_re.captures(&line) {
            let entity_id = caps[1].to_string();

            if current_entity.as_ref() != Some(&entity_id) {
                // Save previous entity to batch
                if let Some(ref prev_entity) = current_entity {
                    entity_batch.push(create_entity(
                        prev_entity,
                        &mut current_metadata,
                        &mut current_triples,
                        &mut entity_claims,
                        &mut entity_type,
                        &mut entity_labels,
                        &mut entity_descriptions,
                        &mut entity_aliases,
                    ));

                    // Process batch when full
                    if entity_batch.len() >= batch_size {
                        let results = process_rdf_batch_parallel(&entity_batch, filter);

                        // Write header once (only for NTriples output)
                        if output_format == OutputFormat::NTriples
                            && !header_written
                            && !results.is_empty()
                        {
                            let header_count = write_header_batch(output, &header_lines)?;
                            triples_output.fetch_add(header_count, Ordering::Relaxed);
                            header_written = true;
                        }

                        // Write results using batch write
                        let (ent_count, triple_count) =
                            write_rdf_output_batch(output, &results, output_format)?;
                        entities_matched.fetch_add(ent_count, Ordering::Relaxed);
                        triples_output.fetch_add(triple_count, Ordering::Relaxed);
                        entity_batch.clear();
                    }
                }

                current_entity = Some(entity_id);
                entity_claims = HashMap::new();
                entity_type = None;
                entity_labels = HashMap::new();
                entity_descriptions = HashMap::new();
                entity_aliases = HashMap::new();
            }

            current_metadata.push(line);
            continue;
        }

        // Parse triple to extract subject entity
        let subject_entity = regexes
            .entity_re
            .captures(&line)
            .map(|caps| caps[1].to_string());

        if let Some(ref entity_id) = subject_entity {
            if current_entity.as_ref() != Some(entity_id) {
                // Save previous entity to batch
                if let Some(ref prev_entity) = current_entity {
                    entity_batch.push(create_entity(
                        prev_entity,
                        &mut current_metadata,
                        &mut current_triples,
                        &mut entity_claims,
                        &mut entity_type,
                        &mut entity_labels,
                        &mut entity_descriptions,
                        &mut entity_aliases,
                    ));

                    // Process batch when full
                    if entity_batch.len() >= batch_size {
                        let results = process_rdf_batch_parallel(&entity_batch, filter);

                        if output_format == OutputFormat::NTriples
                            && !header_written
                            && !results.is_empty()
                        {
                            let header_count = write_header_batch(output, &header_lines)?;
                            triples_output.fetch_add(header_count, Ordering::Relaxed);
                            header_written = true;
                        }

                        let (ent_count, triple_count) =
                            write_rdf_output_batch(output, &results, output_format)?;
                        entities_matched.fetch_add(ent_count, Ordering::Relaxed);
                        triples_output.fetch_add(triple_count, Ordering::Relaxed);
                        entity_batch.clear();
                    }
                }

                current_entity = Some(entity_id.clone());
                entity_claims = HashMap::new();
                entity_type = None;
                entity_labels = HashMap::new();
                entity_descriptions = HashMap::new();
                entity_aliases = HashMap::new();
            }

            // Extract labels, descriptions, aliases
            if regexes.label_re.is_match(&line) {
                if let Some(caps) = regexes.lang_literal_re.captures(&line) {
                    let value = caps[1].to_string();
                    let lang = caps[2].to_string();
                    // Apply language filter
                    if filter.language_filter.is_none() || filter.matches_language(&lang) {
                        entity_labels.insert(lang, value);
                    }
                }
            } else if regexes.description_re.is_match(&line) {
                if let Some(caps) = regexes.lang_literal_re.captures(&line) {
                    let value = caps[1].to_string();
                    let lang = caps[2].to_string();
                    if filter.language_filter.is_none() || filter.matches_language(&lang) {
                        entity_descriptions.insert(lang, value);
                    }
                }
            } else if regexes.alias_re.is_match(&line) {
                if let Some(caps) = regexes.lang_literal_re.captures(&line) {
                    let value = caps[1].to_string();
                    let lang = caps[2].to_string();
                    if filter.language_filter.is_none() || filter.matches_language(&lang) {
                        entity_aliases
                            .entry(lang)
                            .or_insert_with(Vec::new)
                            .push(value);
                    }
                }
            }

            // Extract claims
            if let Some(prop_caps) = regexes.prop_direct_re.captures(&line) {
                let prop_id = prop_caps[1].to_string();
                if let Some(val_caps) = regexes.entity_value_re.captures(&line) {
                    let value_id = val_caps[1].to_string();
                    entity_claims
                        .entry(prop_id.clone())
                        .or_insert_with(HashSet::new)
                        .insert(value_id);
                } else {
                    entity_claims.entry(prop_id).or_insert_with(HashSet::new);
                }
            }

            // Extract entity type
            if line.contains("rdf-syntax-ns#type") {
                if let Some(type_caps) = regexes.type_re.captures(&line) {
                    entity_type = Some(type_caps[1].to_string().to_lowercase());
                }
            }

            // Apply property filter
            if let Some(ref prop_filter) = filter.property_filter {
                if let Some(prop_caps) = regexes.prop_direct_re.captures(&line) {
                    let prop_id = &prop_caps[1];
                    if !prop_filter.contains(prop_id) && !line.contains("rdf-syntax-ns#type") {
                        continue;
                    }
                }
            }

            // Apply language filter to any triple with a language tag
            if filter.language_filter.is_some() {
                if let Some(lang_match) = extract_language_tag(&line) {
                    if !filter.matches_language(&lang_match) {
                        continue;
                    }
                }
            }

            current_triples.push(line);
        }
    }

    // Add last entity to batch
    if let Some(ref entity_id) = current_entity {
        entity_batch.push(create_entity(
            entity_id,
            &mut current_metadata,
            &mut current_triples,
            &mut entity_claims,
            &mut entity_type,
            &mut entity_labels,
            &mut entity_descriptions,
            &mut entity_aliases,
        ));
    }

    // Process remaining batch
    if !entity_batch.is_empty() {
        let results = process_rdf_batch_parallel(&entity_batch, filter);

        if output_format == OutputFormat::NTriples && !header_written && !results.is_empty() {
            let header_count = write_header_batch(output, &header_lines)?;
            triples_output.fetch_add(header_count, Ordering::Relaxed);
        }

        let (ent_count, triple_count) = write_rdf_output_batch(output, &results, output_format)?;
        entities_matched.fetch_add(ent_count, Ordering::Relaxed);
        triples_output.fetch_add(triple_count, Ordering::Relaxed);
    }

    if show_progress {
        eprintln!(
            "Done! Total {} lines, skipped {}, processed {}, matched {} entities, output {} triples",
            lines_processed.load(Ordering::Relaxed),
            lines_skipped.load(Ordering::Relaxed),
            lines_actually_processed,
            entities_matched.load(Ordering::Relaxed),
            triples_output.load(Ordering::Relaxed)
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_entity() -> RdfEntity {
        let mut claims = HashMap::new();
        claims.insert(
            "P31".to_string(),
            HashSet::from(["Q6256".to_string(), "Q3624078".to_string()]),
        );
        claims.insert("P17".to_string(), HashSet::from(["Q183".to_string()]));

        let mut labels = HashMap::new();
        labels.insert("de".to_string(), "Deutschland".to_string());
        labels.insert("en".to_string(), "Germany".to_string());

        let mut descriptions = HashMap::new();
        descriptions.insert("de".to_string(), "Staat in Mitteleuropa".to_string());
        descriptions.insert("en".to_string(), "country in Central Europe".to_string());

        let mut aliases = HashMap::new();
        aliases.insert(
            "de".to_string(),
            vec!["Bundesrepublik Deutschland".to_string(), "BRD".to_string()],
        );
        aliases.insert(
            "en".to_string(),
            vec!["Federal Republic of Germany".to_string()],
        );

        RdfEntity {
            id: "Q183".to_string(),
            metadata: vec![],
            triples: vec![],
            claims,
            entity_type: Some("item".to_string()),
            labels,
            descriptions,
            aliases,
        }
    }

    #[test]
    fn test_rdf_entity_to_json_basic() {
        let entity = create_test_entity();
        let json = rdf_entity_to_json(&entity);

        assert_eq!(json["id"], "Q183");
        assert_eq!(json["type"], "item");
    }

    #[test]
    fn test_rdf_entity_to_json_labels() {
        let entity = create_test_entity();
        let json = rdf_entity_to_json(&entity);

        // Check labels structure
        let labels = json.get("labels").expect("labels should exist");
        let de_label = labels.get("de").expect("de label should exist");
        assert_eq!(de_label["language"], "de");
        assert_eq!(de_label["value"], "Deutschland");

        let en_label = labels.get("en").expect("en label should exist");
        assert_eq!(en_label["language"], "en");
        assert_eq!(en_label["value"], "Germany");
    }

    #[test]
    fn test_rdf_entity_to_json_descriptions() {
        let entity = create_test_entity();
        let json = rdf_entity_to_json(&entity);

        let descriptions = json.get("descriptions").expect("descriptions should exist");
        let de_desc = descriptions.get("de").expect("de description should exist");
        assert_eq!(de_desc["language"], "de");
        assert_eq!(de_desc["value"], "Staat in Mitteleuropa");
    }

    #[test]
    fn test_rdf_entity_to_json_aliases() {
        let entity = create_test_entity();
        let json = rdf_entity_to_json(&entity);

        let aliases = json.get("aliases").expect("aliases should exist");
        let de_aliases = aliases.get("de").expect("de aliases should exist");
        let de_aliases_arr = de_aliases.as_array().expect("de aliases should be array");

        assert_eq!(de_aliases_arr.len(), 2);
        assert_eq!(de_aliases_arr[0]["language"], "de");
        // Values could be in any order due to Vec
        let values: Vec<&str> = de_aliases_arr
            .iter()
            .map(|a| a["value"].as_str().unwrap())
            .collect();
        assert!(values.contains(&"Bundesrepublik Deutschland"));
        assert!(values.contains(&"BRD"));
    }

    #[test]
    fn test_rdf_entity_to_json_claims() {
        let entity = create_test_entity();
        let json = rdf_entity_to_json(&entity);

        let claims = json.get("claims").expect("claims should exist");
        let p31_claims = claims.get("P31").expect("P31 claims should exist");
        let p31_arr = p31_claims.as_array().expect("P31 should be array");

        assert_eq!(p31_arr.len(), 2);

        // Check claim structure
        let first_claim = &p31_arr[0];
        assert_eq!(first_claim["type"], "statement");
        assert_eq!(first_claim["rank"], "normal");

        let mainsnak = &first_claim["mainsnak"];
        assert_eq!(mainsnak["snaktype"], "value");
        assert_eq!(mainsnak["property"], "P31");
        assert_eq!(mainsnak["datavalue"]["type"], "wikibase-entityid");

        let value = &mainsnak["datavalue"]["value"];
        assert_eq!(value["entity-type"], "item");
        // ID could be Q6256 or Q3624078
        let id = value["id"].as_str().unwrap();
        assert!(id == "Q6256" || id == "Q3624078");
    }

    #[test]
    fn test_rdf_entity_to_json_empty_claims_skipped() {
        let mut entity = create_test_entity();
        // Add an empty claim (like a literal-only property)
        entity.claims.insert("P123".to_string(), HashSet::new());

        let json = rdf_entity_to_json(&entity);
        let claims = json.get("claims").expect("claims should exist");

        // Empty claims should be skipped
        assert!(claims.get("P123").is_none());
        // Non-empty claims should still exist
        assert!(claims.get("P31").is_some());
    }

    #[test]
    fn test_rdf_entity_to_json_no_labels() {
        let mut entity = create_test_entity();
        entity.labels.clear();

        let json = rdf_entity_to_json(&entity);

        // labels key should not exist if empty
        assert!(json.get("labels").is_none());
    }

    #[test]
    fn test_rdf_entity_to_json_property_type() {
        let mut entity = create_test_entity();
        entity.id = "P31".to_string();
        entity.entity_type = Some("property".to_string());
        entity.claims.clear();
        entity
            .claims
            .insert("P1628".to_string(), HashSet::from(["P279".to_string()]));

        let json = rdf_entity_to_json(&entity);

        assert_eq!(json["id"], "P31");
        assert_eq!(json["type"], "property");

        // Check that property values have correct entity-type
        let claims = json.get("claims").unwrap();
        let p1628 = claims.get("P1628").unwrap().as_array().unwrap();
        let value = &p1628[0]["mainsnak"]["datavalue"]["value"];
        assert_eq!(value["entity-type"], "property");
    }

    #[test]
    fn test_write_rdf_entities_as_json_batch() {
        let entity = create_test_entity();
        let entities = vec![entity];

        let mut output = Vec::new();
        let result = write_rdf_entities_as_json_batch(&mut output, &entities);

        assert!(result.is_ok());
        let (count, _) = result.unwrap();
        assert_eq!(count, 1);

        // Parse the output as JSON
        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().split('\n').collect();
        assert_eq!(lines.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["id"], "Q183");
    }

    #[test]
    fn test_output_format_enum() {
        assert_eq!(OutputFormat::NTriples, OutputFormat::NTriples);
        assert_eq!(OutputFormat::Json, OutputFormat::Json);
        assert_ne!(OutputFormat::NTriples, OutputFormat::Json);
    }

    #[test]
    fn test_extract_language_tag() {
        assert_eq!(
            extract_language_tag(r#""Germany"@en ."#),
            Some("en".to_string())
        );
        assert_eq!(
            extract_language_tag(r#""Deutschland"@de ."#),
            Some("de".to_string())
        );
        assert_eq!(
            extract_language_tag(r#""Schweiz"@de-ch ."#),
            Some("de-ch".to_string())
        );
        assert_eq!(
            extract_language_tag(r#"<http://example.org/thing> ."#),
            None
        );
    }

    #[test]
    fn test_rdf_regexes_label() {
        let regexes = RdfRegexes::new();

        let label_line = r#"<http://www.wikidata.org/entity/Q183> <http://www.w3.org/2000/01/rdf-schema#label> "Germany"@en ."#;
        assert!(regexes.label_re.is_match(label_line));

        let non_label_line = r#"<http://www.wikidata.org/entity/Q183> <http://www.wikidata.org/prop/direct/P31> <http://www.wikidata.org/entity/Q6256> ."#;
        assert!(!regexes.label_re.is_match(non_label_line));
    }

    #[test]
    fn test_rdf_regexes_description() {
        let regexes = RdfRegexes::new();

        let desc_line = r#"<http://www.wikidata.org/entity/Q183> <http://schema.org/description> "country in Central Europe"@en ."#;
        assert!(regexes.description_re.is_match(desc_line));
    }

    #[test]
    fn test_rdf_regexes_alias() {
        let regexes = RdfRegexes::new();

        let alias_line = r#"<http://www.wikidata.org/entity/Q183> <http://www.w3.org/2004/02/skos/core#altLabel> "Federal Republic of Germany"@en ."#;
        assert!(regexes.alias_re.is_match(alias_line));
    }

    #[test]
    fn test_rdf_regexes_lang_literal() {
        let regexes = RdfRegexes::new();

        let line = r#"<http://www.wikidata.org/entity/Q183> <http://www.w3.org/2000/01/rdf-schema#label> "Deutschland"@de ."#;
        let caps = regexes.lang_literal_re.captures(line);
        assert!(caps.is_some());

        let caps = caps.unwrap();
        assert_eq!(&caps[1], "Deutschland");
        assert_eq!(&caps[2], "de");
    }
}
