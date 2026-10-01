//! Adding a term to the custom dictionary from a chat message. Pure and
//! host-tested; the dictionary file is `{ "category": { "ja": "ko" } }`
//! (see `resonance_core::text::Dictionary`), read and written whole by the
//! `get_local_dictionary` / `save_local_dictionary` commands.

use serde_json::{Map, Value};

/// The category a new term goes into unless another is picked.
pub const DEFAULT_CATEGORY: &str = "chat";

/// Why a term was not added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictError {
    EmptyKey,
    EmptyValue,
    EmptyCategory,
    /// The file is not a JSON object of categories.
    Broken(String),
}

impl DictError {
    pub fn message(&self) -> String {
        match self {
            DictError::EmptyKey => "원문(일본어)을 입력하세요.".to_string(),
            DictError::EmptyValue => "번역(한국어)을 입력하세요.".to_string(),
            DictError::EmptyCategory => "분류를 입력하세요.".to_string(),
            DictError::Broken(why) => format!("사전 파일을 읽을 수 없습니다: {why}"),
        }
    }
}

fn parse(json: &str) -> Result<Map<String, Value>, DictError> {
    if json.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(root)) => Ok(root),
        Ok(_) => Err(DictError::Broken("최상위가 객체가 아닙니다".to_string())),
        Err(e) => Err(DictError::Broken(e.to_string())),
    }
}

/// The categories of the dictionary, [`DEFAULT_CATEGORY`] first (it is offered
/// even when the file has none yet), the rest in file order.
pub fn categories(json: &str) -> Vec<String> {
    let mut found: Vec<String> = parse(json)
        .map(|root| {
            root.into_iter()
                .filter(|(_, terms)| terms.is_object())
                .map(|(name, _)| name)
                .collect()
        })
        .unwrap_or_default();
    found.retain(|c| c != DEFAULT_CATEGORY);
    found.insert(0, DEFAULT_CATEGORY.to_string());
    found
}

/// What `key` already means in `category`, if it is there.
pub fn existing(json: &str, category: &str, key: &str) -> Option<String> {
    let root = parse(json).ok()?;
    root.get(category)?
        .get(key.trim())?
        .as_str()
        .map(str::to_string)
}

/// The dictionary with `key` -> `value` set in `category` (a new term, or the
/// old meaning replaced), as the text to save. Everything else in the file is
/// kept.
pub fn add_entry(json: &str, category: &str, key: &str, value: &str) -> Result<String, DictError> {
    let (category, key, value) = (category.trim(), key.trim(), value.trim());
    if category.is_empty() {
        return Err(DictError::EmptyCategory);
    }
    if key.is_empty() {
        return Err(DictError::EmptyKey);
    }
    if value.is_empty() {
        return Err(DictError::EmptyValue);
    }
    let mut root = parse(json)?;
    let terms = root
        .entry(category.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(terms) = terms.as_object_mut() else {
        return Err(DictError::Broken(format!(
            "'{category}' 분류가 객체가 아닙니다"
        )));
    };
    terms.insert(key.to_string(), Value::String(value.to_string()));
    serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| DictError::Broken(e.to_string()))
}

/// The dialog's starting key and value for a message: the text selected in
/// it (a word of the original is a term to define; a word of the translation
/// is the meaning to give), else the whole original and its translation.
pub fn draft(selection: &str, original: &str, translated: Option<&str>) -> (String, String) {
    let selection = selection.trim();
    if !selection.is_empty() {
        if original.contains(selection) {
            return (selection.to_string(), String::new());
        }
        if translated.is_some_and(|t| t.contains(selection)) {
            return (String::new(), selection.to_string());
        }
    }
    (
        original.trim().to_string(),
        translated.unwrap_or_default().trim().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_term_is_added_to_an_existing_category() {
        let json = r#"{"chat": {"よろしく": "잘 부탁해"}}"#;
        let saved = add_entry(json, "chat", " ありがとう ", " 고마워 ").unwrap();
        assert_eq!(
            parsed(&saved),
            parsed(r#"{"chat": {"よろしく": "잘 부탁해", "ありがとう": "고마워"}}"#)
        );
    }

    #[test]
    fn a_new_category_and_an_empty_file_are_fine() {
        for json in ["", "  ", "{}"] {
            let saved = add_entry(json, "던전", "極限空間", "극한 공간").unwrap();
            assert_eq!(
                parsed(&saved),
                parsed(r#"{"던전": {"極限空間": "극한 공간"}}"#)
            );
        }
    }

    #[test]
    fn an_old_meaning_is_replaced_and_the_rest_of_the_file_kept() {
        let json = r#"{"chat": {"gg": "좋은 게임"}, "던전": {"a": "b"}}"#;
        let saved = add_entry(json, "chat", "gg", "수고했어").unwrap();
        assert_eq!(
            parsed(&saved),
            parsed(r#"{"chat": {"gg": "수고했어"}, "던전": {"a": "b"}}"#)
        );
    }

    #[test]
    fn nothing_is_added_without_a_category_a_key_and_a_value() {
        assert_eq!(add_entry("{}", "chat", "  ", "x"), Err(DictError::EmptyKey));
        assert_eq!(add_entry("{}", "chat", "x", ""), Err(DictError::EmptyValue));
        assert_eq!(
            add_entry("{}", " ", "x", "y"),
            Err(DictError::EmptyCategory)
        );
    }

    #[test]
    fn a_broken_file_is_an_error_not_overwritten() {
        assert!(matches!(
            add_entry("[1]", "chat", "a", "b"),
            Err(DictError::Broken(_))
        ));
        assert!(matches!(
            add_entry("{oops", "chat", "a", "b"),
            Err(DictError::Broken(_))
        ));
        assert!(matches!(
            add_entry(r#"{"chat": 5}"#, "chat", "a", "b"),
            Err(DictError::Broken(_))
        ));
    }

    #[test]
    fn what_a_term_already_means_is_found() {
        let json = r#"{"chat": {"gg": "좋은 게임"}}"#;
        assert_eq!(
            existing(json, "chat", " gg "),
            Some("좋은 게임".to_string())
        );
        assert_eq!(existing(json, "chat", "nope"), None);
        assert_eq!(existing(json, "던전", "gg"), None);
        assert_eq!(existing("{oops", "chat", "gg"), None);
    }

    #[test]
    fn the_default_category_comes_first_even_when_the_file_lacks_it() {
        assert_eq!(categories(""), ["chat"]);
        assert_eq!(
            categories(r#"{"던전": {}, "chat": {}, "직업": {}, "x": 1}"#),
            ["chat", "던전", "직업"]
        );
        assert_eq!(categories("{oops"), ["chat"]);
    }

    #[test]
    fn the_dialog_starts_from_the_selection_else_the_whole_message() {
        let original = "開拓局3日分@4";
        let translated = Some("개척국 3일분 @4");
        assert_eq!(
            draft("開拓局", original, translated),
            ("開拓局".to_string(), String::new()),
            "a word of the original is the term"
        );
        assert_eq!(
            draft("개척국", original, translated),
            (String::new(), "개척국".to_string()),
            "a word of the translation is the meaning"
        );
        let whole = ("開拓局3日分@4".to_string(), "개척국 3일분 @4".to_string());
        assert_eq!(draft("", original, translated), whole);
        assert_eq!(
            draft("elsewhere", original, translated),
            whole,
            "not from this message"
        );
        assert_eq!(
            draft("  ", "gg", None),
            ("gg".to_string(), String::new()),
            "an untranslated message has no value to offer"
        );
    }
}
