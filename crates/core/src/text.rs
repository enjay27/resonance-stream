use lazy_static::lazy_static;
use regex::{Captures, Regex};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct ShieldData {
    pub masked_text: String,
    pub replacements: HashMap<String, String>,
}

lazy_static! {
    static ref RECRUIT_PATTERN: Regex = Regex::new(r"@[A-Za-z0-9]+").unwrap();
    static ref NUM_UNIT_PATTERN: Regex = Regex::new(r"(\d+)(種|人|周|回)").unwrap();
    static ref THINK_PATTERN: Regex = Regex::new(r"(?s)<think>.*?</think>\s*").unwrap();
    static ref TURN_TAG_PATTERN: Regex =
        Regex::new(r"</?end_of_turn>|</?start_of_turn>|<bos>|<eos>").unwrap();
    static ref SPACE_BEFORE_PUNCT: Regex = Regex::new(r"\s+([.!?,~])").unwrap();
    static ref EXTRA_SPACES: Regex = Regex::new(r"\s+").unwrap();
}

/// Japanese brackets are shielded so the model keeps them as they are.
const JP_BRACKETS: [&str; 14] = [
    "【", "】", "「", "」", "『", "』", "（", "）", "〈", "〉", "《", "》", "［", "］",
];
/// What stickers and inline emotes are displayed as (see [`normalize_emotes`]).
pub const STICKER_TOKEN: &str = "[스티커]";
pub const EMOTE_TOKEN: &str = "[이모지]";

fn number_unit_ko(unit: &str) -> &'static str {
    match unit {
        "種" => "종",
        "人" => "인",
        "周" => "주",
        _ => "회", // 回
    }
}

// --- DICTIONARY ---

/// The custom JA -> KO term list, sorted once (longest term first) so each
/// message does not have to sort it again.
#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    entries: Vec<(String, String)>,
}

impl Dictionary {
    /// Parses the categorised dictionary JSON: `{ "category": { "ja": "ko" } }`.
    /// Empty input is an empty dictionary; malformed JSON is an error.
    pub fn from_json_str(content: &str) -> Result<Self, String> {
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        let json: serde_json::Value =
            serde_json::from_str(content).map_err(|e| format!("JSON Syntax Error: {}", e))?;
        let root = json
            .as_object()
            .ok_or_else(|| "Root JSON is not an object.".to_string())?;

        let ignored_brackets = "【】「」『』（）〈〉《》";
        let mut map = HashMap::new();
        for (category, inner_value) in root {
            let Some(inner_obj) = inner_value.as_object() else {
                log::warn!(
                    "[Dictionary] Category '{}' is not a valid object.",
                    category
                );
                continue;
            };
            for (k, v) in inner_obj {
                // Brackets are shielded separately; an empty key would match everywhere.
                if k.is_empty() || ignored_brackets.contains(k.as_str()) {
                    continue;
                }
                if let Some(val_str) = v.as_str() {
                    map.insert(k.clone(), val_str.to_string());
                }
            }
        }
        Ok(Self::from(map))
    }

    /// Loads the dictionary file. Any problem is logged and yields an empty
    /// dictionary, so translation keeps working without it.
    pub fn load(path: &Path) -> Self {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[Dictionary] Failed to read dict file: {}", e);
                return Self::default();
            }
        };
        match Self::from_json_str(&content) {
            Ok(dict) => {
                log::info!("[Dictionary] Loaded {} terms.", dict.len());
                dict
            }
            Err(e) => {
                log::warn!("[Dictionary] {}", e);
                Self::default()
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks a term up (linear; for diagnostics and tests, not the hot path).
    pub fn get(&self, ja: &str) -> Option<&String> {
        self.entries.iter().find(|(k, _)| k == ja).map(|(_, v)| v)
    }

    pub fn contains_key(&self, ja: &str) -> bool {
        self.get(ja).is_some()
    }

    fn entries(&self) -> &[(String, String)] {
        &self.entries
    }
}

impl From<HashMap<String, String>> for Dictionary {
    fn from(map: HashMap<String, String>) -> Self {
        let mut entries: Vec<_> = map.into_iter().filter(|(k, _)| !k.is_empty()).collect();
        // Longest first, so "AliceBob" is shielded before "Alice"; then by key
        // for a deterministic order.
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        Self { entries }
    }
}

// --- MASKING ---

/// Text being shielded: plain text interleaved with placeholders. Terms are
/// only ever searched for in the plain text, so a term can never match
/// inside a placeholder that is already there (e.g. "P0" inside "[P0]").
enum Piece {
    Text(String),
    Placeholder(String),
}

struct Masker {
    pieces: Vec<Piece>,
    replacements: HashMap<String, String>,
    next: usize,
}

impl Masker {
    fn new(input: &str) -> Self {
        Self {
            pieces: vec![Piece::Text(input.to_string())],
            replacements: HashMap::new(),
            next: 0,
        }
    }

    fn contains(&self, target: &str) -> bool {
        self.pieces
            .iter()
            .any(|p| matches!(p, Piece::Text(t) if t.contains(target)))
    }

    fn new_placeholder(&mut self, replacement: String) -> String {
        let placeholder = format!("[P{}]", self.next);
        self.next += 1;
        self.replacements.insert(placeholder.clone(), replacement);
        placeholder
    }

    /// Shields every occurrence of `target` behind one placeholder.
    fn mask_literal(&mut self, target: &str, replacement: &str) {
        if target.is_empty() || !self.contains(target) {
            return;
        }
        let placeholder = self.new_placeholder(replacement.to_string());
        let mut pieces = Vec::with_capacity(self.pieces.len() + 2);
        for piece in std::mem::take(&mut self.pieces) {
            match piece {
                Piece::Text(text) if text.contains(target) => {
                    let mut parts = text.split(target).peekable();
                    while let Some(part) = parts.next() {
                        if !part.is_empty() {
                            pieces.push(Piece::Text(part.to_string()));
                        }
                        if parts.peek().is_some() {
                            pieces.push(Piece::Placeholder(placeholder.clone()));
                        }
                    }
                }
                other => pieces.push(other),
            }
        }
        self.pieces = pieces;
    }

    /// Shields each regex match behind its own placeholder.
    fn mask_regex(&mut self, re: &Regex, replacement: impl Fn(&Captures) -> String) {
        let mut pieces = Vec::with_capacity(self.pieces.len());
        for piece in std::mem::take(&mut self.pieces) {
            let Piece::Text(text) = piece else {
                pieces.push(piece);
                continue;
            };
            let mut last = 0;
            for caps in re.captures_iter(&text) {
                let m = caps.get(0).expect("group 0 always matches");
                if m.start() > last {
                    pieces.push(Piece::Text(text[last..m.start()].to_string()));
                }
                let placeholder = self.new_placeholder(replacement(&caps));
                pieces.push(Piece::Placeholder(placeholder));
                last = m.end();
            }
            if last < text.len() {
                pieces.push(Piece::Text(text[last..].to_string()));
            }
        }
        self.pieces = pieces;
    }

    fn finish(self) -> ShieldData {
        let masked_text = self
            .pieces
            .iter()
            .map(|p| match p {
                Piece::Text(t) | Piece::Placeholder(t) => t.as_str(),
            })
            .collect();
        ShieldData {
            masked_text,
            replacements: self.replacements,
        }
    }
}

// --- PREPROCESSOR ---
pub fn preprocess_text(
    input: &str,
    custom_dict: &Dictionary,
    nickname_cache: Option<&HashMap<String, String>>,
) -> ShieldData {
    let mut masker = Masker::new(input);

    // 0. Emote tokens (display text, not Japanese)
    for token in [STICKER_TOKEN, EMOTE_TOKEN] {
        masker.mask_literal(token, token);
    }

    // 1. Mask Japanese Brackets
    for bracket in JP_BRACKETS {
        masker.mask_literal(bracket, bracket);
    }

    // 2. Replace Nicknames from Cache. The cache grows all session, so only the
    // names present in this message are sorted (longest first).
    if let Some(cache) = nickname_cache {
        let mut names: Vec<(&String, &String)> = cache
            .iter()
            .filter(|(ja_name, _)| input.contains(ja_name.as_str()))
            .collect();
        names.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
        for (ja_name, romaji) in names {
            masker.mask_literal(ja_name, romaji);
        }
    }

    // 3. Recruitment & @-Tag
    let mut recruit_tags: Vec<&str> = RECRUIT_PATTERN
        .find_iter(input)
        .map(|m| m.as_str())
        .collect();
    recruit_tags.dedup();
    for tag in recruit_tags {
        masker.mask_literal(tag, tag);
    }

    // 4. Custom Dictionary Terms (pre-sorted, longest first)
    for (ja, ko) in custom_dict.entries() {
        masker.mask_literal(ja, ko);
    }

    // 5. Numeric Units, one pass for all of them
    masker.mask_regex(&NUM_UNIT_PATTERN, |caps| {
        format!("{}{}", &caps[1], number_unit_ko(&caps[2]))
    });

    masker.finish()
}

// --- POSTPROCESSOR ---
pub fn postprocess_text(translated: &str, shield: &ShieldData) -> String {
    // 1. Strip <think> tags
    let mut final_text = THINK_PATTERN.replace_all(translated, "").to_string();

    // 2. Strip leaked model turn tokens (</end_of_turn> etc.)
    final_text = TURN_TAG_PATTERN.replace_all(&final_text, "").to_string();

    // [P1]이 [P10]의 일부를 먼저 치환해버리는 버그를 막기 위해,
    // 문자열 길이가 긴 것(예: [P10])부터 내림차순 정렬하여 안전하게 치환합니다.
    let mut placeholders: Vec<&String> = shield.replacements.keys().collect();
    placeholders.sort_by(|a, b| b.len().cmp(&a.len()));

    // Restore shielded words safely
    for placeholder in placeholders {
        if let Some(replacement) = shield.replacements.get(placeholder) {
            final_text = final_text.replace(placeholder, replacement);
        }
    }

    // Clean up weird LLM spacing around punctuation
    final_text = SPACE_BEFORE_PUNCT
        .replace_all(&final_text, "$1")
        .to_string();

    // Collapse extra spaces
    EXTRA_SPACES
        .replace_all(&final_text, " ")
        .trim()
        .to_string()
}

/// Display form of stickers and inline emotes: a standalone sticker
/// (`emojiPic=...`) becomes [`STICKER_TOKEN`], each `<sprite=...>` tag
/// becomes [`EMOTE_TOKEN`]. A tag without its closing `>` is left as is.
pub fn normalize_emotes(message: &str) -> String {
    if message.starts_with("emojiPic=") {
        return STICKER_TOKEN.to_string();
    }
    if !message.contains("<sprite=") {
        return message.to_string();
    }
    let mut output = String::with_capacity(message.len());
    let mut current = message;
    while let Some(start) = current.find("<sprite=") {
        output.push_str(&current[..start]);
        match current[start..].find('>') {
            Some(end) => {
                output.push_str(EMOTE_TOKEN);
                current = &current[start + end + 1..];
            }
            None => {
                // Malformed tag: keep the rest verbatim.
                output.push_str(&current[start..]);
                return output;
            }
        }
    }
    output.push_str(current);
    output
}

pub fn convert_to_romaji(ja_name: &str) -> String {
    // 1. kakasi를 이용해 한 번에 Romaji로 변환합니다. (예: "azururu")
    let romaji_str = kakasi::convert(ja_name).romaji;

    // 2. 띄어쓰기가 있다면 단어별로 쪼개서 앞글자만 대문자로 포맷팅합니다.
    romaji_str
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_shield() -> ShieldData {
        ShieldData {
            masked_text: String::new(),
            replacements: HashMap::new(),
        }
    }

    #[test]
    fn test_shielding_pipeline() {
        let mut custom_dict = HashMap::new();
        custom_dict.insert("火力".to_string(), "딜러".to_string());
        custom_dict.insert("完凸".to_string(), "풀돌".to_string());

        let mut nicknames = HashMap::new();
        nicknames.insert("アズルル".to_string(), "Azururu".to_string());

        let original_text = "【火力】@アズルル 完凸 3周 <think>LLM is thinking...</think>";

        // 1. Test Preprocessor
        let custom_dict = Dictionary::from(custom_dict);
        let shield = preprocess_text(original_text, &custom_dict, Some(&nicknames));

        // Ensure the original terms are no longer in the masked text
        assert!(!shield.masked_text.contains("火力"));
        assert!(!shield.masked_text.contains("【"));
        assert!(!shield.masked_text.contains("3周"));

        // Ensure the dictionary captured the correct replacements
        let vals: Vec<&String> = shield.replacements.values().collect();
        assert!(vals.contains(&&"【".to_string()));
        assert!(vals.contains(&&"딜러".to_string()));
        assert!(vals.contains(&&"풀돌".to_string()));
        assert!(vals.contains(&&"3주".to_string())); // 3周 -> 3주

        // 2. Test Postprocessor (Simulating LLM output)
        // We pretend the LLM translated the text but left the [P0] tags intact
        let simulated_llm_output = shield.masked_text.clone();
        let final_result = postprocess_text(&simulated_llm_output, &shield);

        // The <think> tag should be stripped, and placeholders restored
        assert_eq!(final_result, "【딜러】@Azururu 풀돌 3주");
    }

    #[test]
    fn test_processor_edge_cases() {
        let dict = Dictionary::default();
        let nicknames = HashMap::new();

        // Edge Case 1: Completely empty input
        let shield1 = preprocess_text("", &dict, Some(&nicknames));
        assert_eq!(shield1.masked_text, "");
        assert!(shield1.replacements.is_empty());

        // Edge Case 2: Unmatched / Broken <think> tags from LLM
        // If the AI starts a think tag but never finishes it, the regex won't match.
        // It should gracefully ignore it rather than crashing.
        let broken_llm_output = "안녕하세요 <think>this is a broken thought...";
        let final_text = postprocess_text(broken_llm_output, &shield1);
        assert_eq!(final_text, "안녕하세요 <think>this is a broken thought...");

        // Edge Case 3: Only brackets, no text
        let shield2 = preprocess_text("【】", &dict, Some(&nicknames));
        // It should mask the brackets themselves to protect them
        assert!(shield2.masked_text.contains("[P0]"));
        assert!(shield2.masked_text.contains("[P1]"));
    }

    #[test]
    fn test_nickname_replacement() {
        let dict = Dictionary::default();
        let mut nicknames = HashMap::new();
        nicknames.insert("あずるる".to_string(), "Azururu".to_string());

        // Standard Case: The player's Japanese name is in the chat
        let original_text = "あずるるさん、こんにちは！";
        let shield = preprocess_text(original_text, &dict, Some(&nicknames));

        // It should be shielded so the AI doesn't translate it
        assert!(shield.masked_text.contains("[P0]"));
        assert!(!shield.masked_text.contains("あずるる"));

        let final_text = postprocess_text(&shield.masked_text, &shield);
        assert_eq!(final_text, "Azururuさん、こんにちは！");
    }

    #[test]
    fn test_nickname_edge_cases() {
        let dict = Dictionary::default();
        let mut nicknames = HashMap::new();
        nicknames.insert("あずる".to_string(), "Azuru".to_string());
        nicknames.insert("アズルル".to_string(), "Azururu".to_string());

        // Edge Case 1: Nickname in cache, but does not exist in the chat message
        let shield1 = preprocess_text("パーティー 구합니다", &dict, Some(&nicknames));
        assert_eq!(shield1.masked_text, "パーティー 구합니다");

        // Edge Case 2: Multiple occurrences of the nickname in one message
        let shield2 = preprocess_text("あずる! アズルル?", &dict, Some(&nicknames));
        assert!(shield2.masked_text.contains("[P1]! [P0]?"));

        let final_text = postprocess_text(&shield2.masked_text, &shield2);
        assert_eq!(final_text, "Azuru! Azururu?");

        // Edge Case 3: No cache provided
        let shield3 = preprocess_text("あずるるさん", &dict, None);
        assert_eq!(shield3.masked_text, "あずるるさん");
    }

    #[test]
    fn test_load_dictionary_success() {
        use std::env;
        use std::fs;

        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("test_custom_dict.json");

        // 1. Create a mock categorized JSON file
        let valid_json = r#"{
            "chat": {
                "disco": "디코",
                "hello": "안녕",
                "【ignored】": "should not load"
            },
            "game": {
                "PT": "파티"
            },
            "invalid_category": "this is a string, not an object",
            "mixed": {
                "number": 123,
                "nested": { "a": "b" },
                "valid": "정상"
            }
        }"#;

        fs::write(&file_path, valid_json).unwrap();

        // 2. Load the dictionary
        let dict = Dictionary::load(&file_path);

        // 3. Assert Standard Success Cases
        assert_eq!(dict.get("disco").map(|s| s.as_str()), Some("디코"));
        assert_eq!(dict.get("hello").map(|s| s.as_str()), Some("안녕"));
        assert_eq!(dict.get("PT").map(|s| s.as_str()), Some("파티"));
        assert_eq!(dict.get("valid").map(|s| s.as_str()), Some("정상"));

        // 4. Assert Edge Case: Ignored Brackets

        // 5. Assert Edge Case: Non-String Values & Invalid Categories
        assert!(
            !dict.contains_key("number"),
            "Numeric values should be skipped"
        );
        assert!(
            !dict.contains_key("nested"),
            "Nested objects should be skipped"
        );
        assert!(
            !dict.contains_key("invalid_category"),
            "Invalid categories should be skipped"
        );

        // 6. Clean up the temp file
        let _ = fs::remove_file(&file_path);
    }

    #[test]
    fn test_load_dictionary_edge_cases() {
        use std::env;
        use std::fs;

        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("test_dict_edge_cases.json");

        // Edge Case 1: Empty File
        fs::write(&file_path, "   ").unwrap();
        let dict_empty = Dictionary::load(&file_path);
        assert!(
            dict_empty.is_empty(),
            "Empty file should return empty HashMap"
        );

        // Edge Case 2: Invalid JSON Syntax
        fs::write(&file_path, "{ broken json...").unwrap();
        let dict_broken = Dictionary::load(&file_path);
        assert!(
            dict_broken.is_empty(),
            "Broken JSON should return empty HashMap"
        );

        // Edge Case 3: Missing File Path
        let missing_path = temp_dir.join("does_not_exist_12345.json");
        let dict_missing = Dictionary::load(&missing_path);
        assert!(
            dict_missing.is_empty(),
            "Missing file should return empty HashMap"
        );

        // Clean up
        let _ = fs::remove_file(&file_path);
    }

    #[test]
    fn test_strips_end_of_turn_closing() {
        // Standard observed case: 장미석</end_of_turn>
        assert_eq!(
            postprocess_text("장미석</end_of_turn>", &empty_shield()),
            "장미석"
        );
    }

    #[test]
    fn test_strips_end_of_turn_no_slash() {
        assert_eq!(
            postprocess_text("안녕<end_of_turn>", &empty_shield()),
            "안녕"
        );
    }

    #[test]
    fn test_strips_start_of_turn_leakage() {
        assert_eq!(
            postprocess_text("<start_of_turn>안녕", &empty_shield()),
            "안녕"
        );
    }

    #[test]
    fn test_strips_bos_eos_tokens() {
        assert_eq!(
            postprocess_text("<bos>번역 결과<eos>", &empty_shield()),
            "번역 결과"
        );
    }

    #[test]
    fn test_strips_multiple_tags_in_one_output() {
        assert_eq!(
            postprocess_text("<bos>번역 결과</end_of_turn>", &empty_shield()),
            "번역 결과"
        );
    }

    #[test]
    fn test_turn_tag_with_placeholder_restoration() {
        // Tag leaked alongside a placeholder that must be restored
        let mut replacements = HashMap::new();
        replacements.insert("[P0]".to_string(), "장미석".to_string());
        let shield = ShieldData {
            masked_text: String::new(),
            replacements,
        };
        assert_eq!(postprocess_text("[P0]</end_of_turn>", &shield), "장미석");
    }

    #[test]
    fn test_turn_tag_injected_mid_sentence() {
        // Defensive: prompt injection inserts tag mid-output
        assert_eq!(
            postprocess_text("안녕</end_of_turn>하세요", &empty_shield()),
            "안녕하세요"
        );
    }

    #[test]
    fn test_clean_output_unchanged() {
        // No tags — output must pass through unmodified
        assert_eq!(
            postprocess_text("오늘 날씨가 좋네요.", &empty_shield()),
            "오늘 날씨가 좋네요."
        );
    }

    #[test]
    fn test_think_and_turn_tag_combined() {
        // Both <think> block and </end_of_turn> present simultaneously
        assert_eq!(
            postprocess_text(
                "<think>내부 추론...</think>번역 결과</end_of_turn>",
                &empty_shield()
            ),
            "번역 결과"
        );
    }

    #[test]
    fn test_only_tags_no_content() {
        // Edge: output is nothing but tags — should produce empty string
        assert_eq!(postprocess_text("</end_of_turn>", &empty_shield()), "");
    }

    #[test]
    fn dictionary_term_never_matches_inside_a_placeholder() {
        // Regression: brackets become "[P0]"/"[P1]"; the key "P0" then matched
        // inside "[P0]" and corrupted it, losing the opening bracket.
        let mut dict = HashMap::new();
        dict.insert("P0".to_string(), "피제로".to_string());
        let shield = preprocess_text("【test】", &dict.into(), None);
        assert_eq!(postprocess_text(&shield.masked_text, &shield), "【test】");
    }

    #[test]
    fn all_number_units_are_shielded() {
        let shield = preprocess_text("3種 2人 5周 1回", &HashMap::new().into(), None);
        assert!(!shield.masked_text.contains(['種', '人', '周', '回']));
        assert_eq!(
            postprocess_text(&shield.masked_text, &shield),
            "3종 2인 5주 1회"
        );
    }

    #[test]
    fn emote_tokens_survive_translation() {
        let shield = preprocess_text("こんにちは[이모지][스티커]", &HashMap::new().into(), None);
        assert!(!shield.masked_text.contains("[이모지]"));
        assert!(!shield.masked_text.contains("[스티커]"));
        // The model copies placeholders through; they come back as the tokens.
        let translated = shield.masked_text.replace("こんにちは", "안녕하세요");
        assert_eq!(
            postprocess_text(&translated, &shield),
            "안녕하세요[이모지][스티커]"
        );
    }

    #[test]
    fn dictionary_prefers_the_longest_term() {
        let mut map = HashMap::new();
        map.insert("火".to_string(), "불".to_string());
        map.insert("火力".to_string(), "딜러".to_string());
        let dict = Dictionary::from(map);
        let shield = preprocess_text("火力と火", &dict, None);
        let restored = postprocess_text(&shield.masked_text, &shield);
        assert_eq!(restored, "딜러と불");
    }

    #[test]
    fn dictionary_from_json_str() {
        let dict = Dictionary::from_json_str(
            r#"{"chat": {"disco": "디코", "【": "x", "": "empty"}, "bad": 1}"#,
        )
        .unwrap();
        assert_eq!(dict.len(), 1);
        assert_eq!(dict.get("disco").map(|s| s.as_str()), Some("디코"));
        assert!(Dictionary::from_json_str("{ broken").is_err());
        assert!(Dictionary::from_json_str("  ").unwrap().is_empty());
    }

    #[test]
    fn normalize_emotes_rewrites_stickers_and_sprites() {
        assert_eq!(normalize_emotes("emojiPic=123_45"), "[스티커]");
        assert_eq!(
            normalize_emotes("やった<sprite=12>ね<sprite=\"a\">"),
            "やった[이모지]ね[이모지]"
        );
        assert_eq!(normalize_emotes("broken <sprite=1"), "broken <sprite=1");
        assert_eq!(normalize_emotes("plain"), "plain");
        // Idempotent: normalized text stays as it is.
        assert_eq!(normalize_emotes("[스티커]"), "[스티커]");
    }

    #[test]
    fn test_placeholder_high_index_not_clobbered_by_low() {
        // [P1] must not partially replace [P10] before [P10] is restored
        let mut replacements = HashMap::new();
        replacements.insert("[P1]".to_string(), "A".to_string());
        replacements.insert("[P10]".to_string(), "B".to_string());
        let shield = ShieldData {
            masked_text: String::new(),
            replacements,
        };
        let result = postprocess_text("[P10] [P1]", &shield);
        assert_eq!(result, "B A");
    }
}
