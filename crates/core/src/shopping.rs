//! Shopping lists (M8, KONZEPT §10): sections of the shop, item names and merging duplicates.
//!
//! Adding items never earns XP; the shopping itself is an "auf Abruf" task
//! (`Schedule::OnCall`) that the list triggers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_NAME: usize = 60;
pub const MAX_NOTE: usize = 80;

/// Where an item is in the shop; the shopping mode groups by this, in this order.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Produce,
    Bakery,
    Chilled,
    Frozen,
    Pantry,
    Drinks,
    Drugstore,
    Household,
    #[default]
    Other,
}

impl Section {
    pub const ALL: [Section; 9] = [
        Section::Produce,
        Section::Bakery,
        Section::Chilled,
        Section::Frozen,
        Section::Pantry,
        Section::Drinks,
        Section::Drugstore,
        Section::Household,
        Section::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Section::Produce => "produce",
            Section::Bakery => "bakery",
            Section::Chilled => "chilled",
            Section::Frozen => "frozen",
            Section::Pantry => "pantry",
            Section::Drinks => "drinks",
            Section::Drugstore => "drugstore",
            Section::Household => "household",
            Section::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Section {
        Section::ALL.into_iter().find(|x| x.as_str() == s).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ItemError {
    #[error("Der Artikel braucht einen Namen (bis {MAX_NAME} Zeichen).")]
    Name,
    #[error("Die Notiz darf höchstens {MAX_NOTE} Zeichen lang sein.")]
    Note,
}

/// Cleans up what someone typed: trimmed, single spaces, first letter upper case
/// ("  milch " → "Milch").
pub fn clean_name(raw: &str) -> Result<String, ItemError> {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = joined.chars().count();
    if n == 0 || n > MAX_NAME {
        return Err(ItemError::Name);
    }
    let mut chars = joined.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    Ok(first + chars.as_str())
}

pub fn clean_note(raw: Option<&str>) -> Result<Option<String>, ItemError> {
    let Some(raw) = raw else { return Ok(None) };
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        return Ok(None);
    }
    if joined.chars().count() > MAX_NOTE {
        return Err(ItemError::Note);
    }
    Ok(Some(joined))
}

/// The key two entries are "the same item" by: lower case, letters and digits only
/// ("Milch", " milch ", "MILCH!" → "milch").
pub fn item_key(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// "Milch" is already on the list and someone adds "Milch, 2 Liter": the notes are combined
/// instead of a second line. Repeated notes are not doubled.
pub fn merge_note(existing: Option<&str>, added: Option<&str>) -> Option<String> {
    match (existing, added) {
        (None, None) => None,
        (Some(e), None) => Some(e.to_string()),
        (None, Some(a)) => Some(a.to_string()),
        (Some(e), Some(a)) if e.to_lowercase().contains(&a.to_lowercase()) => Some(e.to_string()),
        (Some(e), Some(a)) => {
            let both = format!("{e} · {a}");
            Some(if both.chars().count() > MAX_NOTE { e.to_string() } else { both })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  milch  laktosefrei ").unwrap(), "Milch laktosefrei");
        assert_eq!(clean_name("äpfel").unwrap(), "Äpfel");
        assert_eq!(clean_name("   "), Err(ItemError::Name));
        assert_eq!(clean_name(&"x".repeat(61)), Err(ItemError::Name));
        assert_eq!(clean_note(Some("  2  Liter ")).unwrap().as_deref(), Some("2 Liter"));
        assert_eq!(clean_note(Some(" ")).unwrap(), None);
        assert_eq!(clean_note(Some(&"x".repeat(81))), Err(ItemError::Note));
    }

    #[test]
    fn same_item_by_key() {
        assert_eq!(item_key("Milch"), item_key(" MILCH! "));
        assert_ne!(item_key("Milch"), item_key("Hafermilch"));
        assert_eq!(item_key("Brötchen"), "brötchen");
    }

    #[test]
    fn notes_merge_without_doubling() {
        assert_eq!(merge_note(None, Some("2 Liter")).as_deref(), Some("2 Liter"));
        assert_eq!(merge_note(Some("2 Liter"), Some("laktosefrei")).as_deref(), Some("2 Liter · laktosefrei"));
        assert_eq!(merge_note(Some("2 Liter"), Some("2 liter")).as_deref(), Some("2 Liter"));
        assert_eq!(merge_note(Some("2 Liter"), None).as_deref(), Some("2 Liter"));
        assert_eq!(merge_note(None, None), None);
    }

    #[test]
    fn sections_round_trip() {
        for s in Section::ALL {
            assert_eq!(Section::parse(s.as_str()), s);
        }
        assert_eq!(Section::parse("unknown"), Section::Other);
        assert_eq!(serde_json::to_string(&Section::Drugstore).unwrap(), r#""drugstore""#);
    }
}
