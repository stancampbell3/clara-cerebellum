use crate::error::RitualError;
use uuid::Uuid;

/// Build a Kafka-safe topic name for a Ritual.
///
/// Format: `{dis_domain}.ritual.{ritual_id}`
///
/// Forward slashes in `dis_domain` are replaced with dots before
/// validation so that domain strings like `"dis/local"` are accepted.
/// All other characters must satisfy Kafka's topic-name constraints
/// (alphanumeric, `.`, `-`, `_`; ≤ 249 chars; not `.` or `..`).
pub fn topic_name(dis_domain: &str, ritual_id: Uuid) -> Result<String, RitualError> {
    let domain = dis_domain.replace('/', ".");
    let name = format!("{}.ritual.{}", domain, ritual_id);
    validate_topic_name(&name)?;
    Ok(name)
}

/// Build a Kafka-safe topic name for an ad hoc, non-Ritual topic.
///
/// Format: `{dis_domain}.coire.{subject_path}`
///
/// The `coire` segment is a fixed, non-ritual designation that segregates
/// these freeform topics from Ritual topics (`{dis_domain}.ritual.{uuid}`)
/// sharing the same Dis domain namespace. Forward slashes in both
/// `dis_domain` and `subject_path` are replaced with dots (so a caller can
/// write a hierarchical path like `"research/edge-detection"`), then the
/// result is validated against the same Kafka naming constraints as
/// [`topic_name`].
pub fn coire_topic_name(dis_domain: &str, subject_path: &str) -> Result<String, RitualError> {
    let domain = dis_domain.replace('/', ".");
    let subject = subject_path.replace('/', ".");
    let name = format!("{}.coire.{}", domain, subject);
    validate_topic_name(&name)?;
    Ok(name)
}

pub(crate) fn validate_topic_name(name: &str) -> Result<(), RitualError> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(RitualError::InvalidTopicName(format!(
            "topic name cannot be empty, '.', or '..': {:?}",
            name
        )));
    }
    if name.len() > 249 {
        return Err(RitualError::InvalidTopicName(format!(
            "topic name exceeds 249 characters ({} chars): {:?}",
            name.len(),
            name
        )));
    }
    for ch in name.chars() {
        if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_') {
            return Err(RitualError::InvalidTopicName(format!(
                "illegal character {:?} in topic name {:?}",
                ch, name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn fixed() -> Uuid {
        Uuid::parse_str(FIXED_ID).unwrap()
    }

    #[test]
    fn basic_topic_name() {
        let name = topic_name("dis.local", fixed()).unwrap();
        assert_eq!(
            name,
            format!("dis.local.ritual.{}", FIXED_ID)
        );
    }

    #[test]
    fn slash_in_domain_replaced_with_dot() {
        let name = topic_name("dis/local/node", fixed()).unwrap();
        assert!(name.starts_with("dis.local.node.ritual."));
    }

    #[test]
    fn reserved_names_rejected() {
        assert!(validate_topic_name(".").is_err());
        assert!(validate_topic_name("..").is_err());
        assert!(validate_topic_name("").is_err());
    }

    #[test]
    fn illegal_characters_rejected() {
        assert!(validate_topic_name("dis local").is_err());   // space
        assert!(validate_topic_name("dis/local").is_err());   // slash (not via topic_name())
        assert!(validate_topic_name("topic@node").is_err());  // @
    }

    #[test]
    fn too_long_rejected() {
        assert!(validate_topic_name(&"a".repeat(250)).is_err());
    }

    #[test]
    fn exactly_249_accepted() {
        assert!(validate_topic_name(&"a".repeat(249)).is_ok());
    }

    #[test]
    fn valid_characters_accepted() {
        assert!(validate_topic_name("dis.local.ritual.550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(validate_topic_name("my_domain.ritual.abc123").is_ok());
        assert!(validate_topic_name("Dis-Node_1.ritual.xyz").is_ok());
    }

    // ── coire_topic_name ─────────────────────────────────────────────────────

    #[test]
    fn basic_coire_topic_name() {
        let name = coire_topic_name("dis.local", "research.edge-detection").unwrap();
        assert_eq!(name, "dis.local.coire.research.edge-detection");
    }

    #[test]
    fn coire_topic_name_normalizes_slashes_in_both_parts() {
        let name = coire_topic_name("dis/local/node", "research/edge-detection").unwrap();
        assert_eq!(name, "dis.local.node.coire.research.edge-detection");
    }

    #[test]
    fn coire_topic_name_distinct_from_ritual_topic_name() {
        let coire = coire_topic_name("dis.local", "abc").unwrap();
        let ritual = topic_name("dis.local", fixed()).unwrap();
        assert!(coire.contains(".coire."));
        assert!(ritual.contains(".ritual."));
    }

    #[test]
    fn coire_topic_name_rejects_illegal_characters() {
        assert!(coire_topic_name("dis.local", "bad subject").is_err());
        assert!(coire_topic_name("dis.local", "bad@subject").is_err());
    }
}
