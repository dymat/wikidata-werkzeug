/// Represents an N-Triples line (subject predicate object .)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct NTriple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub raw: String,
}

#[allow(dead_code)]
impl NTriple {
    /// Parse an N-Triples line
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        // N-Triples format: <subject> <predicate> <object> .
        // or: <subject> <predicate> "literal"^^<type> .
        // or: <subject> <predicate> "literal"@lang .

        let mut parts: Vec<&str> = Vec::new();
        let mut current_start = 0;
        let mut in_uri = false;
        let mut in_literal = false;
        let mut escape_next = false;

        let chars: Vec<char> = line.chars().collect();
        for (i, &ch) in chars.iter().enumerate() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match ch {
                '\\' => escape_next = true,
                '<' if !in_literal => in_uri = true,
                '>' if !in_literal && in_uri => {
                    in_uri = false;
                    if current_start < i + 1 {
                        parts.push(&line[current_start..=i]);
                        current_start = i + 1;
                    }
                }
                '"' if !in_uri => {
                    if !in_literal {
                        in_literal = true;
                        if current_start < i {
                            current_start = i;
                        }
                    } else {
                        in_literal = false;
                        // Include potential type/lang suffix
                    }
                }
                ' ' | '\t' if !in_uri && !in_literal => {
                    if current_start < i {
                        let part = &line[current_start..i];
                        if !part.trim().is_empty() && part.trim() != "." {
                            parts.push(part.trim());
                        }
                    }
                    current_start = i + 1;
                }
                _ => {}
            }
        }

        // Add remaining part
        if current_start < line.len() {
            let part = &line[current_start..];
            let part = part.trim().trim_end_matches('.');
            if !part.trim().is_empty() {
                parts.push(part.trim());
            }
        }

        if parts.len() >= 3 {
            Some(NTriple {
                subject: parts[0].to_string(),
                predicate: parts[1].to_string(),
                object: parts[2..]
                    .join(" ")
                    .trim_end_matches(" .")
                    .trim_end_matches('.')
                    .trim()
                    .to_string(),
                raw: line.to_string(),
            })
        } else {
            None
        }
    }

    /// Extract entity ID from a Wikidata URI (e.g., Q31 from <http://www.wikidata.org/entity/Q31>)
    pub fn extract_entity_id(uri: &str) -> Option<String> {
        if uri.contains("wikidata.org/entity/") {
            uri.rsplit('/')
                .next()
                .map(|s| s.trim_end_matches('>').to_string())
        } else {
            None
        }
    }

    /// Extract property ID from a Wikidata property URI
    pub fn extract_property_id(uri: &str) -> Option<String> {
        if uri.contains("wikidata.org/prop/direct/")
            || uri.contains("wikidata.org/prop/direct-normalized/")
        {
            uri.rsplit('/')
                .next()
                .map(|s| s.trim_end_matches('>').to_string())
        } else {
            None
        }
    }

    /// Get the subject entity ID if this is a Wikidata entity triple
    pub fn subject_entity_id(&self) -> Option<String> {
        Self::extract_entity_id(&self.subject)
    }

    /// Get the predicate property ID if this is a direct property triple
    pub fn predicate_property_id(&self) -> Option<String> {
        Self::extract_property_id(&self.predicate)
    }

    /// Get the object entity ID if the object is a Wikidata entity
    pub fn object_entity_id(&self) -> Option<String> {
        Self::extract_entity_id(&self.object)
    }

    /// Check if this triple defines the type of an entity
    pub fn is_type_triple(&self) -> bool {
        self.predicate.contains("rdf-syntax-ns#type")
    }

    /// Get the entity type if this is a type triple
    pub fn entity_type(&self) -> Option<String> {
        if self.is_type_triple() {
            if self.object.contains("wikiba.se/ontology#Item") {
                Some("item".to_string())
            } else if self.object.contains("wikiba.se/ontology#Property") {
                Some("property".to_string())
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_entity_triple() {
        let line = r#"<http://www.wikidata.org/entity/Q31> <http://www.wikidata.org/prop/direct/P31> <http://www.wikidata.org/entity/Q6256> ."#;
        let triple = NTriple::parse(line).unwrap();

        assert!(triple.subject.contains("Q31"));
        assert!(triple.predicate.contains("P31"));
        assert!(triple.object.contains("Q6256"));
    }

    #[test]
    fn test_parse_literal_triple() {
        let line = r#"<http://www.wikidata.org/entity/Q31> <http://www.wikidata.org/prop/direct/P1082> "+11825551"^^<http://www.w3.org/2001/XMLSchema#decimal> ."#;
        let triple = NTriple::parse(line).unwrap();

        assert!(triple.subject.contains("Q31"));
        assert!(triple.object.contains("11825551"));
    }

    #[test]
    fn test_extract_entity_id() {
        assert_eq!(
            NTriple::extract_entity_id("<http://www.wikidata.org/entity/Q31>"),
            Some("Q31".to_string())
        );
        assert_eq!(
            NTriple::extract_entity_id("<http://www.wikidata.org/entity/Q6256>"),
            Some("Q6256".to_string())
        );
    }

    #[test]
    fn test_extract_property_id() {
        assert_eq!(
            NTriple::extract_property_id("<http://www.wikidata.org/prop/direct/P31>"),
            Some("P31".to_string())
        );
    }
}
