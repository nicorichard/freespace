// Filtering logic for module and item lists.

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
}
