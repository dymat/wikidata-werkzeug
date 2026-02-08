use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::FilterError;

/// Valid entity attributes that can be filtered with --keep/--omit
pub const VALID_ATTRIBUTES: &[&str] = &[
    "id",
    "type",
    "labels",
    "descriptions",
    "aliases",
    "claims",
    "sitelinks",
];

/// Parse --keep and --omit attribute filters
/// Returns (keep_attributes, omit_attributes)
pub fn parse_attribute_filters(
    keep: Option<&str>,
    omit: Option<&str>,
) -> Result<(Option<HashSet<String>>, Option<HashSet<String>>), FilterError> {
    // Validate that keep and omit are not both specified
    if keep.is_some() && omit.is_some() {
        return Err(FilterError::Parse(
            "Cannot use both --keep and --omit at the same time".to_string(),
        ));
    }

    let parse_attrs = |s: &str| -> Result<HashSet<String>, FilterError> {
        let attrs: HashSet<String> = s
            .split(',')
            .map(|a| a.trim().to_lowercase())
            .filter(|a| !a.is_empty())
            .collect();

        // Validate all attributes
        for attr in &attrs {
            if !VALID_ATTRIBUTES.contains(&attr.as_str()) {
                return Err(FilterError::Parse(format!(
                    "Invalid attribute '{}'. Valid attributes: {}",
                    attr,
                    VALID_ATTRIBUTES.join(", ")
                )));
            }
        }

        Ok(attrs)
    };

    let keep_attrs = keep.map(parse_attrs).transpose()?;
    let omit_attrs = omit.map(parse_attrs).transpose()?;

    Ok((keep_attrs, omit_attrs))
}

/// Represents a claim filter condition
#[derive(Debug, Clone)]
pub enum ClaimFilter {
    /// Property has any value (e.g., P18)
    HasProperty(String),
    /// Property has specific value(s) (e.g., P31:Q5 or P31:Q5,Q6256)
    PropertyValue(String, HashSet<String>),
    /// AND of multiple filters (e.g., P31:Q5&P18)
    And(Vec<ClaimFilter>),
    /// OR of multiple filters (e.g., P31:Q5|P31:Q6256)
    Or(Vec<ClaimFilter>),
    /// NOT filter (e.g., ~P31:Q5)
    Not(Box<ClaimFilter>),
}

impl ClaimFilter {
    /// Check if the filter matches the given claims
    pub fn matches(&self, claims: &HashMap<String, HashSet<String>>) -> bool {
        match self {
            ClaimFilter::HasProperty(prop) => claims.contains_key(prop),

            ClaimFilter::PropertyValue(prop, values) => {
                if let Some(claim_values) = claims.get(prop) {
                    // Check if any of the required values are in the claim values
                    values.iter().any(|v| claim_values.contains(v))
                } else {
                    false
                }
            }

            ClaimFilter::And(filters) => filters.iter().all(|f| f.matches(claims)),

            ClaimFilter::Or(filters) => filters.iter().any(|f| f.matches(claims)),

            ClaimFilter::Not(filter) => !filter.matches(claims),
        }
    }
}

/// Main entity filter configuration
#[derive(Debug, Clone)]
pub struct EntityFilter {
    pub claim_filter: Option<ClaimFilter>,
    pub subject_filter: Option<HashSet<String>>,
    pub property_filter: Option<HashSet<String>>,
    pub language_filter: Option<HashSet<String>>,
    pub language_include_subvariants: bool,
    pub entity_type: String,
    /// Attributes to keep (if Some, only these attributes are kept)
    pub keep_attributes: Option<HashSet<String>>,
    /// Attributes to omit (if Some, these attributes are removed)
    pub omit_attributes: Option<HashSet<String>>,
}

impl EntityFilter {
    /// Check if a language tag matches the language filter
    pub fn matches_language(&self, lang_tag: &str) -> bool {
        if let Some(ref lang_filter) = self.language_filter {
            if self.language_include_subvariants {
                // Extract base language (e.g., "de" from "de-ch")
                let base_lang = lang_tag.split('-').next().unwrap_or(lang_tag);
                lang_filter.contains(lang_tag) || lang_filter.contains(base_lang)
            } else {
                lang_filter.contains(lang_tag)
            }
        } else {
            true
        }
    }

    /// Check if an RDF entity matches all filters
    pub fn matches(
        &self,
        entity_id: &str,
        claims: &HashMap<String, HashSet<String>>,
        entity_type: Option<&str>,
    ) -> bool {
        // Check subject filter
        if let Some(ref subjects) = self.subject_filter {
            if !subjects.contains(entity_id) {
                return false;
            }
        }

        // Check entity type filter
        if self.entity_type != "both" {
            if let Some(etype) = entity_type {
                if etype != self.entity_type {
                    return false;
                }
            }
        }

        // Check claim filter
        if let Some(ref filter) = self.claim_filter {
            if !filter.matches(claims) {
                return false;
            }
        }

        true
    }

    /// Check if a JSON entity matches all filters
    pub fn matches_json(&self, entity: &Value) -> bool {
        // Get entity ID
        let entity_id = entity.get("id").and_then(|v| v.as_str()).unwrap_or("");

        // Check subject filter
        if let Some(ref subjects) = self.subject_filter {
            if !subjects.contains(entity_id) {
                return false;
            }
        }

        // Check entity type
        if self.entity_type != "both" {
            let etype = entity
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("item");
            if etype != self.entity_type {
                return false;
            }
        }

        // Check claim filter
        if let Some(ref filter) = self.claim_filter {
            let claims = self.extract_json_claims(entity);
            if !filter.matches(&claims) {
                return false;
            }
        }

        true
    }

    /// Extract claims from a JSON entity into the same format used for RDF
    fn extract_json_claims(&self, entity: &Value) -> HashMap<String, HashSet<String>> {
        let mut claims: HashMap<String, HashSet<String>> = HashMap::new();

        if let Some(claims_obj) = entity.get("claims").and_then(|c| c.as_object()) {
            for (prop_id, statements) in claims_obj {
                let mut values = HashSet::new();

                if let Some(statements_arr) = statements.as_array() {
                    for statement in statements_arr {
                        // Get the main snak value
                        if let Some(mainsnak) = statement.get("mainsnak") {
                            if let Some(datavalue) = mainsnak.get("datavalue") {
                                if let Some(value_obj) = datavalue.get("value") {
                                    // Entity reference
                                    if let Some(entity_id) =
                                        value_obj.get("id").and_then(|v| v.as_str())
                                    {
                                        values.insert(entity_id.to_string());
                                    }
                                    // Numeric ID (older format)
                                    else if let Some(numeric_id) =
                                        value_obj.get("numeric-id").and_then(|v| v.as_u64())
                                    {
                                        let entity_type = value_obj
                                            .get("entity-type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("item");
                                        let prefix =
                                            if entity_type == "property" { "P" } else { "Q" };
                                        values.insert(format!("{}{}", prefix, numeric_id));
                                    }
                                }
                            }
                        }
                    }
                }

                claims.insert(prop_id.clone(), values);
            }
        }

        claims
    }

    /// Check if an attribute should be included in the output
    fn should_include_attribute(&self, attr: &str) -> bool {
        if let Some(ref keep) = self.keep_attributes {
            // If keep is specified, only include listed attributes
            keep.contains(attr)
        } else if let Some(ref omit) = self.omit_attributes {
            // If omit is specified, exclude listed attributes
            !omit.contains(attr)
        } else {
            // No filter, include everything
            true
        }
    }

    /// Filter a JSON entity to keep only requested data
    pub fn filter_json_entity(&self, entity: &Value) -> Value {
        let obj = match entity.as_object() {
            Some(o) => o,
            None => return entity.clone(),
        };

        let mut result = serde_json::Map::new();

        // Process each attribute based on keep/omit filters
        for (key, value) in obj {
            if !self.should_include_attribute(key) {
                continue;
            }

            let mut filtered_value = value.clone();

            // Apply language filter to language-specific attributes
            if let Some(ref langs) = self.language_filter {
                match key.as_str() {
                    "labels" | "descriptions" | "aliases" => {
                        if let Some(lang_map) = filtered_value.as_object_mut() {
                            lang_map.retain(|k, _| langs.contains(k));
                        }
                    }
                    "sitelinks" => {
                        // Sitelinks use language codes as part of the key (e.g., "enwiki", "dewiki")
                        // We could filter these too, but typically sitelinks are filtered differently
                    }
                    _ => {}
                }
            }

            // Apply property filter to claims
            if key == "claims" {
                if let Some(ref props) = self.property_filter {
                    if let Some(claims_map) = filtered_value.as_object_mut() {
                        claims_map.retain(|k, _| props.contains(k));
                    }
                }
            }

            result.insert(key.clone(), filtered_value);
        }

        Value::Object(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_property_filter() {
        let filter = ClaimFilter::HasProperty("P31".to_string());

        let mut claims = HashMap::new();
        claims.insert("P31".to_string(), HashSet::from(["Q5".to_string()]));

        assert!(filter.matches(&claims));

        let empty_claims = HashMap::new();
        assert!(!filter.matches(&empty_claims));
    }

    #[test]
    fn test_property_value_filter() {
        let filter = ClaimFilter::PropertyValue(
            "P31".to_string(),
            HashSet::from(["Q5".to_string(), "Q6256".to_string()]),
        );

        let mut claims = HashMap::new();
        claims.insert("P31".to_string(), HashSet::from(["Q5".to_string()]));
        assert!(filter.matches(&claims));

        claims.insert("P31".to_string(), HashSet::from(["Q6256".to_string()]));
        assert!(filter.matches(&claims));

        claims.insert("P31".to_string(), HashSet::from(["Q123".to_string()]));
        assert!(!filter.matches(&claims));
    }

    #[test]
    fn test_and_filter() {
        let filter = ClaimFilter::And(vec![
            ClaimFilter::HasProperty("P31".to_string()),
            ClaimFilter::HasProperty("P18".to_string()),
        ]);

        let mut claims = HashMap::new();
        claims.insert("P31".to_string(), HashSet::from(["Q5".to_string()]));
        assert!(!filter.matches(&claims));

        claims.insert("P18".to_string(), HashSet::new());
        assert!(filter.matches(&claims));
    }

    #[test]
    fn test_or_filter() {
        let filter = ClaimFilter::Or(vec![
            ClaimFilter::PropertyValue("P31".to_string(), HashSet::from(["Q5".to_string()])),
            ClaimFilter::PropertyValue("P31".to_string(), HashSet::from(["Q6256".to_string()])),
        ]);

        let mut claims = HashMap::new();
        claims.insert("P31".to_string(), HashSet::from(["Q5".to_string()]));
        assert!(filter.matches(&claims));

        claims.insert("P31".to_string(), HashSet::from(["Q6256".to_string()]));
        assert!(filter.matches(&claims));

        claims.insert("P31".to_string(), HashSet::from(["Q123".to_string()]));
        assert!(!filter.matches(&claims));
    }

    #[test]
    fn test_not_filter() {
        let filter = ClaimFilter::Not(Box::new(ClaimFilter::PropertyValue(
            "P31".to_string(),
            HashSet::from(["Q5".to_string()]),
        )));

        let mut claims = HashMap::new();
        claims.insert("P31".to_string(), HashSet::from(["Q5".to_string()]));
        assert!(!filter.matches(&claims));

        claims.insert("P31".to_string(), HashSet::from(["Q6256".to_string()]));
        assert!(filter.matches(&claims));
    }

    #[test]
    fn test_language_filter_exact_match() {
        let filter = EntityFilter {
            claim_filter: None,
            subject_filter: None,
            property_filter: None,
            language_filter: Some(HashSet::from(["de".to_string(), "en".to_string()])),
            language_include_subvariants: false,
            entity_type: "item".to_string(),
            keep_attributes: None,
            omit_attributes: None,
        };

        // Exact matches
        assert!(filter.matches_language("de"));
        assert!(filter.matches_language("en"));

        // Subvariants should NOT match with exact_match
        assert!(!filter.matches_language("de-ch"));
        assert!(!filter.matches_language("de-at"));
        assert!(!filter.matches_language("en-gb"));
        assert!(!filter.matches_language("en-us"));

        // Other languages
        assert!(!filter.matches_language("fr"));
        assert!(!filter.matches_language("es"));
    }

    #[test]
    fn test_language_filter_with_subvariants() {
        let filter = EntityFilter {
            claim_filter: None,
            subject_filter: None,
            property_filter: None,
            language_filter: Some(HashSet::from(["de".to_string(), "en".to_string()])),
            language_include_subvariants: true,
            entity_type: "item".to_string(),
            keep_attributes: None,
            omit_attributes: None,
        };

        // Exact matches
        assert!(filter.matches_language("de"));
        assert!(filter.matches_language("en"));

        // Subvariants SHOULD match
        assert!(filter.matches_language("de-ch"));
        assert!(filter.matches_language("de-at"));
        assert!(filter.matches_language("en-gb"));
        assert!(filter.matches_language("en-us"));

        // Other languages still don't match
        assert!(!filter.matches_language("fr"));
        assert!(!filter.matches_language("fr-ca"));
        assert!(!filter.matches_language("es"));
    }

    #[test]
    fn test_language_filter_none() {
        let filter = EntityFilter {
            claim_filter: None,
            subject_filter: None,
            property_filter: None,
            language_filter: None,
            language_include_subvariants: true,
            entity_type: "item".to_string(),
            keep_attributes: None,
            omit_attributes: None,
        };

        // Without language filter, everything matches
        assert!(filter.matches_language("de"));
        assert!(filter.matches_language("en"));
        assert!(filter.matches_language("fr"));
        assert!(filter.matches_language("de-ch"));
    }

    #[test]
    fn test_keep_attributes() {
        let filter = EntityFilter {
            claim_filter: None,
            subject_filter: None,
            property_filter: None,
            language_filter: None,
            language_include_subvariants: true,
            entity_type: "item".to_string(),
            keep_attributes: Some(HashSet::from(["id".to_string(), "labels".to_string()])),
            omit_attributes: None,
        };

        let entity: Value = serde_json::from_str(
            r#"{
            "id": "Q42",
            "type": "item",
            "labels": {"en": {"language": "en", "value": "Douglas Adams"}},
            "descriptions": {"en": {"language": "en", "value": "English author"}},
            "claims": {}
        }"#,
        )
        .unwrap();

        let filtered = filter.filter_json_entity(&entity);
        let obj = filtered.as_object().unwrap();

        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("labels"));
        assert!(!obj.contains_key("type"));
        assert!(!obj.contains_key("descriptions"));
        assert!(!obj.contains_key("claims"));
    }

    #[test]
    fn test_omit_attributes() {
        let filter = EntityFilter {
            claim_filter: None,
            subject_filter: None,
            property_filter: None,
            language_filter: None,
            language_include_subvariants: true,
            entity_type: "item".to_string(),
            keep_attributes: None,
            omit_attributes: Some(HashSet::from([
                "claims".to_string(),
                "sitelinks".to_string(),
            ])),
        };

        let entity: Value = serde_json::from_str(
            r#"{
            "id": "Q42",
            "type": "item",
            "labels": {"en": {"language": "en", "value": "Douglas Adams"}},
            "claims": {},
            "sitelinks": {}
        }"#,
        )
        .unwrap();

        let filtered = filter.filter_json_entity(&entity);
        let obj = filtered.as_object().unwrap();

        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("type"));
        assert!(obj.contains_key("labels"));
        assert!(!obj.contains_key("claims"));
        assert!(!obj.contains_key("sitelinks"));
    }

    #[test]
    fn test_parse_attribute_filters_valid() {
        let (keep, omit) = parse_attribute_filters(Some("id,labels,descriptions"), None).unwrap();
        assert!(keep.is_some());
        assert!(omit.is_none());
        let keep_set = keep.unwrap();
        assert!(keep_set.contains("id"));
        assert!(keep_set.contains("labels"));
        assert!(keep_set.contains("descriptions"));
    }

    #[test]
    fn test_parse_attribute_filters_invalid() {
        let result = parse_attribute_filters(Some("id,invalid_attr"), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_attribute_filters_both_error() {
        let result = parse_attribute_filters(Some("id"), Some("claims"));
        assert!(result.is_err());
    }
}
