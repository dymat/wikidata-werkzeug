use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rayon::prelude::*;

use crate::filter::EntityFilter;
use crate::rdf::OutputFormat;
use crate::FilterError;

/// Convert a JSON entity to N-Triples format
pub fn json_entity_to_ntriples(entity: &serde_json::Value) -> Vec<String> {
    let mut triples = Vec::new();

    let entity_id = match entity.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return triples,
    };

    let subject = format!("<http://www.wikidata.org/entity/{}>", entity_id);

    // Entity type
    if let Some(etype) = entity.get("type").and_then(|v| v.as_str()) {
        let type_uri = match etype {
            "item" => "<http://wikiba.se/ontology#Item>",
            "property" => "<http://wikiba.se/ontology#Property>",
            _ => "<http://wikiba.se/ontology#Item>",
        };
        triples.push(format!(
            "{} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> {} .",
            subject, type_uri
        ));
    }

    // Labels
    if let Some(labels) = entity.get("labels").and_then(|v| v.as_object()) {
        for (lang, label_obj) in labels {
            if let Some(value) = label_obj.get("value").and_then(|v| v.as_str()) {
                let escaped = escape_ntriples_string(value);
                triples.push(format!(
                    "{} <http://www.w3.org/2000/01/rdf-schema#label> \"{}\"@{} .",
                    subject, escaped, lang
                ));
            }
        }
    }

    // Descriptions
    if let Some(descriptions) = entity.get("descriptions").and_then(|v| v.as_object()) {
        for (lang, desc_obj) in descriptions {
            if let Some(value) = desc_obj.get("value").and_then(|v| v.as_str()) {
                let escaped = escape_ntriples_string(value);
                triples.push(format!(
                    "{} <http://schema.org/description> \"{}\"@{} .",
                    subject, escaped, lang
                ));
            }
        }
    }

    // Aliases
    if let Some(aliases) = entity.get("aliases").and_then(|v| v.as_object()) {
        for (lang, alias_arr) in aliases {
            if let Some(arr) = alias_arr.as_array() {
                for alias_obj in arr {
                    if let Some(value) = alias_obj.get("value").and_then(|v| v.as_str()) {
                        let escaped = escape_ntriples_string(value);
                        triples.push(format!(
                            "{} <http://www.w3.org/2004/02/skos/core#altLabel> \"{}\"@{} .",
                            subject, escaped, lang
                        ));
                    }
                }
            }
        }
    }

    // Claims
    if let Some(claims) = entity.get("claims").and_then(|v| v.as_object()) {
        for (prop_id, statements) in claims {
            if let Some(statements_arr) = statements.as_array() {
                for statement in statements_arr {
                    if let Some(mainsnak) = statement.get("mainsnak") {
                        if let Some(datavalue) = mainsnak.get("datavalue") {
                            let datatype = datavalue.get("type").and_then(|v| v.as_str());

                            if let Some(value_obj) = datavalue.get("value") {
                                let predicate =
                                    format!("<http://www.wikidata.org/prop/direct/{}>", prop_id);

                                match datatype {
                                    Some("wikibase-entityid") => {
                                        // Entity reference
                                        if let Some(target_id) =
                                            value_obj.get("id").and_then(|v| v.as_str())
                                        {
                                            triples.push(format!(
                                                "{} {} <http://www.wikidata.org/entity/{}> .",
                                                subject, predicate, target_id
                                            ));
                                        }
                                    }
                                    Some("string") => {
                                        if let Some(s) = value_obj.as_str() {
                                            let escaped = escape_ntriples_string(s);
                                            triples.push(format!(
                                                "{} {} \"{}\" .",
                                                subject, predicate, escaped
                                            ));
                                        }
                                    }
                                    Some("monolingualtext") => {
                                        if let Some(text) =
                                            value_obj.get("text").and_then(|v| v.as_str())
                                        {
                                            let lang = value_obj
                                                .get("language")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("en");
                                            let escaped = escape_ntriples_string(text);
                                            triples.push(format!(
                                                "{} {} \"{}\"@{} .",
                                                subject, predicate, escaped, lang
                                            ));
                                        }
                                    }
                                    Some("quantity") => {
                                        if let Some(amount) =
                                            value_obj.get("amount").and_then(|v| v.as_str())
                                        {
                                            triples.push(format!(
                                                "{} {} \"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal> .",
                                                subject, predicate, amount
                                            ));
                                        }
                                    }
                                    Some("time") => {
                                        if let Some(time) =
                                            value_obj.get("time").and_then(|v| v.as_str())
                                        {
                                            triples.push(format!(
                                                "{} {} \"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime> .",
                                                subject, predicate, time
                                            ));
                                        }
                                    }
                                    Some("globecoordinate") => {
                                        if let (Some(lat), Some(lon)) = (
                                            value_obj.get("latitude").and_then(|v| v.as_f64()),
                                            value_obj.get("longitude").and_then(|v| v.as_f64()),
                                        ) {
                                            triples.push(format!(
                                                "{} {} \"Point({} {})\"^^<http://www.opengis.net/ont/geosparql#wktLiteral> .",
                                                subject, predicate, lon, lat
                                            ));
                                        }
                                    }
                                    _ => {
                                        // For other types, try to serialize as string
                                        if let Some(s) = value_obj.as_str() {
                                            let escaped = escape_ntriples_string(s);
                                            triples.push(format!(
                                                "{} {} \"{}\" .",
                                                subject, predicate, escaped
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    triples
}

/// Escape special characters for N-Triples string literals
pub fn escape_ntriples_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
}

/// Write JSON entities efficiently using batch writes
fn write_json_batch<W: Write>(output: &mut W, json_lines: &[String]) -> std::io::Result<()> {
    if json_lines.is_empty() {
        return Ok(());
    }

    // Calculate total size for efficient allocation
    let total_size: usize = json_lines.iter().map(|s| s.len() + 1).sum();
    let mut buffer = String::with_capacity(total_size);

    for line in json_lines {
        buffer.push_str(line);
        buffer.push('\n');
    }

    output.write_all(buffer.as_bytes())
}

/// Process a batch of JSON lines in parallel
fn process_json_batch_parallel(
    batch: &[String],
    filter: &Arc<EntityFilter>,
    show_progress: bool,
    output_format: OutputFormat,
) -> Vec<String> {
    batch
        .par_iter()
        .filter_map(
            |line| match serde_json::from_str::<serde_json::Value>(line) {
                Ok(entity) => {
                    if filter.matches_json(&entity) {
                        let filtered_entity = filter.filter_json_entity(&entity);
                        match output_format {
                            OutputFormat::Json => serde_json::to_string(&filtered_entity).ok(),
                            OutputFormat::NTriples => {
                                let triples = json_entity_to_ntriples(&filtered_entity);
                                if triples.is_empty() {
                                    None
                                } else {
                                    Some(triples.join("\n"))
                                }
                            }
                        }
                    } else {
                        None
                    }
                }
                Err(e) => {
                    if show_progress {
                        eprintln!("Warning: Failed to parse JSON: {}", e);
                    }
                    None
                }
            },
        )
        .collect()
}

/// Main JSON filtering function with parallel processing
pub fn filter_json_parallel<R: BufRead, W: Write>(
    reader: R,
    output: &mut W,
    filter: &Arc<EntityFilter>,
    show_progress: bool,
    batch_size: usize,
    skip_lines: u64,
    max_lines: u64,
    output_format: OutputFormat,
) -> Result<(), FilterError> {
    let lines_processed = AtomicU64::new(0);
    let lines_skipped = AtomicU64::new(0);
    let entities_matched = AtomicU64::new(0);
    let mut skip_mode = skip_lines > 0;

    let mut line_batch: Vec<String> = Vec::with_capacity(batch_size);

    for line_result in reader.lines() {
        let line = line_result?;
        let current_line = lines_processed.fetch_add(1, Ordering::Relaxed) + 1;

        // Check max_lines limit
        if current_line > skip_lines + max_lines {
            if show_progress {
                eprintln!("Reached max_lines limit ({}), stopping.", max_lines);
            }
            break;
        }

        // Skip lines if needed
        if skip_mode {
            lines_skipped.fetch_add(1, Ordering::Relaxed);
            if current_line >= skip_lines {
                skip_mode = false;
                if show_progress {
                    eprintln!(
                        "Skipped {} lines, starting processing at line {}...",
                        skip_lines, current_line
                    );
                }
            }
            continue;
        }

        if show_progress && (current_line - skip_lines) % 10000 == 0 {
            eprintln!(
                "Processed {} lines (skipped {}), matched {} entities",
                current_line,
                lines_skipped.load(Ordering::Relaxed),
                entities_matched.load(Ordering::Relaxed)
            );
        }

        // Skip empty lines and array brackets
        let trimmed = line.trim().trim_end_matches(',');
        if trimmed.is_empty() || trimmed == "[" || trimmed == "]" {
            continue;
        }

        line_batch.push(trimmed.to_string());

        // Process batch when full
        if line_batch.len() >= batch_size {
            let results =
                process_json_batch_parallel(&line_batch, filter, show_progress, output_format);
            entities_matched.fetch_add(results.len() as u64, Ordering::Relaxed);

            write_json_batch(output, &results)?;
            line_batch.clear();
        }
    }

    // Process remaining batch
    if !line_batch.is_empty() {
        let results =
            process_json_batch_parallel(&line_batch, filter, show_progress, output_format);
        entities_matched.fetch_add(results.len() as u64, Ordering::Relaxed);

        write_json_batch(output, &results)?;
    }

    if show_progress {
        eprintln!(
            "Done! Processed {} lines, matched {} entities",
            lines_processed.load(Ordering::Relaxed),
            entities_matched.load(Ordering::Relaxed)
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_entity_to_ntriples_basic() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"id":"Q42","type":"item","labels":{"en":{"language":"en","value":"Douglas Adams"}}}"#,
        )
        .unwrap();

        let triples = json_entity_to_ntriples(&json);

        assert!(triples.len() >= 2);
        assert!(triples
            .iter()
            .any(|t| t.contains("Q42") && t.contains("rdf-syntax-ns#type")));
        assert!(triples
            .iter()
            .any(|t| t.contains("Douglas Adams") && t.contains("@en")));
    }

    #[test]
    fn test_json_entity_to_ntriples_claims() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"id":"Q42","type":"item","claims":{"P31":[{"mainsnak":{"snaktype":"value","property":"P31","datavalue":{"value":{"entity-type":"item","id":"Q5"},"type":"wikibase-entityid"}}}]}}"#,
        )
        .unwrap();

        let triples = json_entity_to_ntriples(&json);

        assert!(triples
            .iter()
            .any(|t| t.contains("prop/direct/P31") && t.contains("entity/Q5")));
    }

    #[test]
    fn test_json_entity_to_ntriples_descriptions() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"id":"Q42","type":"item","descriptions":{"de":{"language":"de","value":"britischer Autor"}}}"#,
        )
        .unwrap();

        let triples = json_entity_to_ntriples(&json);

        assert!(triples
            .iter()
            .any(|t| t.contains("schema.org/description") && t.contains("britischer Autor")));
    }

    #[test]
    fn test_json_entity_to_ntriples_aliases() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"id":"Q42","type":"item","aliases":{"en":[{"language":"en","value":"DNA"}]}}"#,
        )
        .unwrap();

        let triples = json_entity_to_ntriples(&json);

        assert!(triples
            .iter()
            .any(|t| t.contains("skos/core#altLabel") && t.contains("DNA")));
    }

    #[test]
    fn test_escape_ntriples_string() {
        assert_eq!(escape_ntriples_string("hello"), "hello");
        assert_eq!(escape_ntriples_string("hello\"world"), "hello\\\"world");
        assert_eq!(escape_ntriples_string("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_ntriples_string("path\\file"), "path\\\\file");
        assert_eq!(escape_ntriples_string("tab\there"), "tab\\there");
    }
}
