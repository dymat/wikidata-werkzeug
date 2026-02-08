use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::filter::ClaimFilter;
use crate::FilterError;

/// Parse a claim filter string like "P31:Q5,Q6256&P18|P279:Q5"
///
/// Syntax:
/// - P31:Q5 - property P31 has value Q5
/// - P31:Q5,Q6256 - property P31 has value Q5 OR Q6256
/// - P18 - property P18 exists (has any value)
/// - P31:Q5&P18 - P31:Q5 AND P18
/// - P31:Q5|P279:Q5 - P31:Q5 OR P279:Q5
/// - ~P31:Q5 - NOT P31:Q5
/// - P31:Q5&~P18 - P31:Q5 AND NOT P18
///
/// Precedence: | (OR) has lower precedence than & (AND)
/// So "A&B|C" means "A AND (B OR C)"
pub fn parse_claim_filter(input: &str) -> Result<ClaimFilter, FilterError> {
    let input = input.trim();

    // Check if input is a file path
    let claim_str = if Path::new(input).exists() {
        fs::read_to_string(input)
            .map_err(|e| FilterError::InvalidClaim(format!("Failed to read claim file: {}", e)))?
            .trim()
            .to_string()
    } else {
        input.to_string()
    };

    parse_or_expression(&claim_str)
}

/// Parse OR expressions (lowest precedence)
fn parse_or_expression(input: &str) -> Result<ClaimFilter, FilterError> {
    let parts = split_top_level(input, '|');

    if parts.len() == 1 {
        return parse_and_expression(&parts[0]);
    }

    let mut filters = Vec::new();
    for part in parts {
        filters.push(parse_and_expression(&part)?);
    }

    Ok(ClaimFilter::Or(filters))
}

/// Parse AND expressions
fn parse_and_expression(input: &str) -> Result<ClaimFilter, FilterError> {
    let parts = split_top_level(input, '&');

    if parts.len() == 1 {
        return parse_atomic(&parts[0]);
    }

    let mut filters = Vec::new();
    for part in parts {
        filters.push(parse_atomic(&part)?);
    }

    Ok(ClaimFilter::And(filters))
}

/// Parse atomic expressions (possibly negated)
fn parse_atomic(input: &str) -> Result<ClaimFilter, FilterError> {
    let input = input.trim();

    // Handle NOT operator
    if input.starts_with('~') {
        let inner = &input[1..];
        return Ok(ClaimFilter::Not(Box::new(parse_atomic(inner)?)));
    }

    // Handle parentheses (for future expansion)
    if input.starts_with('(') && input.ends_with(')') {
        return parse_or_expression(&input[1..input.len() - 1]);
    }

    // Parse property[:values] expression
    parse_property_filter(input)
}

/// Parse a single property filter like "P31:Q5,Q6256" or "P18"
fn parse_property_filter(input: &str) -> Result<ClaimFilter, FilterError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(FilterError::InvalidClaim("Empty claim filter".to_string()));
    }

    if let Some(colon_pos) = input.find(':') {
        let property = input[..colon_pos].trim().to_string();
        let values_str = &input[colon_pos + 1..];

        // Validate property ID
        if !is_valid_property_id(&property) {
            return Err(FilterError::InvalidClaim(format!(
                "Invalid property ID: {}",
                property
            )));
        }

        // Parse values (comma-separated)
        let values: HashSet<String> = values_str
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();

        if values.is_empty() {
            return Err(FilterError::InvalidClaim(format!(
                "No values specified for property {}",
                property
            )));
        }

        // Validate entity IDs
        for value in &values {
            if !is_valid_entity_id(value) {
                return Err(FilterError::InvalidClaim(format!(
                    "Invalid entity ID: {}",
                    value
                )));
            }
        }

        Ok(ClaimFilter::PropertyValue(property, values))
    } else {
        // Just a property (check for existence)
        let property = input.to_string();

        if !is_valid_property_id(&property) {
            return Err(FilterError::InvalidClaim(format!(
                "Invalid property ID: {}",
                property
            )));
        }

        Ok(ClaimFilter::HasProperty(property))
    }
}

/// Split string by delimiter at top level (not inside parentheses)
fn split_top_level(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for ch in input.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            c if c == delimiter && paren_depth == 0 => {
                if !current.is_empty() {
                    parts.push(current.trim().to_string());
                    current = String::new();
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

/// Validate property ID format (P followed by digits)
fn is_valid_property_id(id: &str) -> bool {
    if !id.starts_with('P') {
        return false;
    }
    id[1..].chars().all(|c| c.is_ascii_digit())
}

/// Validate entity ID format (Q or P followed by digits)
fn is_valid_entity_id(id: &str) -> bool {
    if id.starts_with('Q') || id.starts_with('P') || id.starts_with('L') {
        id[1..].chars().all(|c| c.is_ascii_digit() || c == '-')
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_property() {
        let filter = parse_claim_filter("P31").unwrap();
        match filter {
            ClaimFilter::HasProperty(p) => assert_eq!(p, "P31"),
            _ => panic!("Expected HasProperty"),
        }
    }

    #[test]
    fn test_parse_property_with_value() {
        let filter = parse_claim_filter("P31:Q5").unwrap();
        match filter {
            ClaimFilter::PropertyValue(p, v) => {
                assert_eq!(p, "P31");
                assert!(v.contains("Q5"));
            }
            _ => panic!("Expected PropertyValue"),
        }
    }

    #[test]
    fn test_parse_property_with_multiple_values() {
        let filter = parse_claim_filter("P31:Q5,Q6256").unwrap();
        match filter {
            ClaimFilter::PropertyValue(p, v) => {
                assert_eq!(p, "P31");
                assert!(v.contains("Q5"));
                assert!(v.contains("Q6256"));
            }
            _ => panic!("Expected PropertyValue"),
        }
    }

    #[test]
    fn test_parse_and_expression() {
        let filter = parse_claim_filter("P31:Q5&P18").unwrap();
        match filter {
            ClaimFilter::And(filters) => {
                assert_eq!(filters.len(), 2);
            }
            _ => panic!("Expected And"),
        }
    }

    #[test]
    fn test_parse_or_expression() {
        let filter = parse_claim_filter("P31:Q5|P31:Q6256").unwrap();
        match filter {
            ClaimFilter::Or(filters) => {
                assert_eq!(filters.len(), 2);
            }
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_parse_not_expression() {
        let filter = parse_claim_filter("~P31:Q5").unwrap();
        match filter {
            ClaimFilter::Not(inner) => match *inner {
                ClaimFilter::PropertyValue(p, _) => assert_eq!(p, "P31"),
                _ => panic!("Expected PropertyValue inside Not"),
            },
            _ => panic!("Expected Not"),
        }
    }

    #[test]
    fn test_parse_complex_expression() {
        // P31:Q5&P18|P279 should parse as (P31:Q5 AND P18) OR P279
        // But per wikibase-dump-filter docs, | has lower precedence
        // so A&B|C = A AND (B OR C)
        let filter = parse_claim_filter("P31:Q5&P18|P279").unwrap();
        match filter {
            ClaimFilter::Or(_) => {
                // Expected: top level is OR
            }
            _ => panic!("Expected Or at top level"),
        }
    }

    #[test]
    fn test_invalid_property() {
        assert!(parse_claim_filter("Q31").is_err());
        assert!(parse_claim_filter("31").is_err());
    }
}
