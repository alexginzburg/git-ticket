use rand::RngCore;

pub fn generate_id() -> String {
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixError {
    NotFound,
    Ambiguous(Vec<String>),
}

pub fn resolve_prefix(prefix: &str, ids: &[String]) -> Result<String, PrefixError> {
    let matches: Vec<String> = ids.iter().filter(|id| id.starts_with(prefix)).cloned().collect();
    match matches.len() {
        0 => Err(PrefixError::NotFound),
        1 => Ok(matches[0].clone()),
        _ => Err(PrefixError::Ambiguous(matches)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_is_16_lowercase_hex_chars() {
        let id = generate_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn two_generated_ids_differ() {
        assert_ne!(generate_id(), generate_id());
    }

    #[test]
    fn resolve_prefix_finds_unique_match() {
        let ids = vec!["abc123".to_string(), "def456".to_string()];
        assert_eq!(resolve_prefix("abc", &ids), Ok("abc123".to_string()));
    }

    #[test]
    fn resolve_prefix_errors_when_not_found() {
        let ids = vec!["abc123".to_string()];
        assert_eq!(resolve_prefix("zzz", &ids), Err(PrefixError::NotFound));
    }

    #[test]
    fn resolve_prefix_errors_when_ambiguous() {
        let ids = vec!["abc123".to_string(), "abc789".to_string()];
        match resolve_prefix("abc", &ids) {
            Err(PrefixError::Ambiguous(mut matches)) => {
                matches.sort();
                assert_eq!(matches, vec!["abc123".to_string(), "abc789".to_string()]);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn resolve_prefix_accepts_full_id_as_its_own_prefix() {
        let ids = vec!["abc123".to_string()];
        assert_eq!(resolve_prefix("abc123", &ids), Ok("abc123".to_string()));
    }
}
