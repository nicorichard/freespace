// Filtering logic for module and item lists.

use crate::module::manifest::{RestoreKind, RiskLevel};

/// Case-insensitive substring match for filtering lists.
pub fn matches_filter(haystack: &str, tags: &[String], query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    if let Some(tag_query) = query.strip_prefix('#') {
        if tag_query.is_empty() {
            return true;
        }
        let tq = tag_query.to_lowercase();
        return tags.iter().any(|t| t.to_lowercase().contains(&tq));
    }
    let q = query.to_lowercase();
    haystack.to_lowercase().contains(&q) || tags.iter().any(|t| t.to_lowercase().contains(&q))
}

/// Check if an item passes the structured filter (risk level + restore kind).
pub fn matches_structured_filter(
    risk: RiskLevel,
    restore: RestoreKind,
    filter_risk: &[bool; 4],
    filter_restore: &[bool; 2],
) -> bool {
    let risk_ok = match risk {
        RiskLevel::Safe => filter_risk[0],
        RiskLevel::Low => filter_risk[1],
        RiskLevel::Medium => filter_risk[2],
        RiskLevel::High => filter_risk[3],
    };
    let restore_ok = match restore {
        RestoreKind::Auto => filter_restore[0],
        RestoreKind::Manual => filter_restore[1],
    };
    risk_ok && restore_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_filter_empty_query() {
        assert!(matches_filter("anything", &[], ""));
    }

    #[test]
    fn matches_filter_case_insensitive() {
        assert!(matches_filter("Docker", &[], "docker"));
        assert!(matches_filter("docker", &[], "DOCK"));
    }

    #[test]
    fn matches_filter_no_match() {
        assert!(!matches_filter("docker", &[], "npm"));
    }

    #[test]
    fn matches_filter_tag_hash_prefix() {
        let tags = vec!["cache".to_string(), "ios".to_string()];
        assert!(matches_filter("docker", &tags, "#cache"));
        assert!(!matches_filter("docker", &tags, "#build-artifacts"));
    }

    #[test]
    fn matches_filter_tag_hash_empty() {
        assert!(matches_filter("docker", &[], "#"));
    }

    #[test]
    fn matches_filter_tag_case_insensitive() {
        let tags = vec!["cache".to_string()];
        assert!(matches_filter("docker", &tags, "#CA"));
    }

    #[test]
    fn matches_filter_plain_query_matches_tags() {
        let tags = vec!["cache".to_string()];
        assert!(matches_filter("docker", &tags, "cache"));
    }

    #[test]
    fn structured_filter_all_enabled() {
        assert!(matches_structured_filter(
            RiskLevel::High,
            RestoreKind::Manual,
            &[true; 4],
            &[true; 2],
        ));
    }

    #[test]
    fn structured_filter_risk_disabled() {
        assert!(!matches_structured_filter(
            RiskLevel::High,
            RestoreKind::Auto,
            &[true, true, true, false],
            &[true; 2],
        ));
    }

    #[test]
    fn structured_filter_restore_disabled() {
        assert!(!matches_structured_filter(
            RiskLevel::Safe,
            RestoreKind::Manual,
            &[true; 4],
            &[true, false],
        ));
    }

    #[test]
    fn structured_filter_both_must_match() {
        assert!(matches_structured_filter(
            RiskLevel::Medium,
            RestoreKind::Auto,
            &[true, true, true, true],
            &[true, false],
        ));
        assert!(!matches_structured_filter(
            RiskLevel::Medium,
            RestoreKind::Manual,
            &[true, true, true, true],
            &[true, false],
        ));
    }
}
