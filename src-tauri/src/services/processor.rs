use std::collections::HashMap;
use std::fs;
use std::path::Path;
use regex::{Regex, Captures};
use lazy_static::lazy_static;

pub struct ShieldData {
    pub masked_text: String,
    pub replacements: HashMap<String, String>,
}

lazy_static! {
    static ref RECRUIT_PATTERN: Regex = Regex::new(r"@[A-Za-z0-9]+").unwrap();
    static ref NUM_PATTERN_1: Regex = Regex::new(r"(\d+)種").unwrap();
    static ref NUM_PATTERN_2: Regex = Regex::new(r"(\d+)人").unwrap();
    static ref NUM_PATTERN_3: Regex = Regex::new(r"(\d+)周").unwrap();
    static ref NUM_PATTERN_4: Regex = Regex::new(r"(\d+)回").unwrap();
    static ref THINK_PATTERN: Regex = Regex::new(r"(?s)<think>.*?</think>\s*").unwrap();
}

// --- PREPROCESSOR ---
pub fn preprocess_text(
    input: &str,
    custom_dict: &HashMap<String, String>,
    nickname_romaji: Option<&str>,
    original_nickname: Option<&str>
) -> ShieldData {
    let mut current_text = input.to_string();
    let mut replacements = HashMap::new();
    let mut p_count = 0;

    // Helper closure to mask terms
    let mut mask_term = |target: &str, replacement: &str, text: &mut String| {
        let placeholder = format!("[P{}]", p_count);
        *text = text.replace(target, &placeholder);
        replacements.insert(placeholder, replacement.to_string());
        p_count += 1;
    };

    // --- NEW: 1. Mask Japanese Brackets ---
    // 특수 괄호들을 먼저 마스킹하여 LLM이 건드리지 못하게 보호합니다.
    let jp_brackets = [
        "【", "】", "「", "」", "『", "』", "（", "）", "〈", "〉", "《", "》", "［", "］"
    ];
    for bracket in jp_brackets {
        if current_text.contains(bracket) {
            mask_term(bracket, bracket, &mut current_text);
        }
    }

    // 2. Replace Nickname if matched
    if let (Some(romaji), Some(ja_name)) = (nickname_romaji, original_nickname) {
        if current_text.contains(ja_name) {
            current_text = current_text.replace(ja_name, romaji);
        }
    }

    // 3. Recruitment & @-Tag (Find all matches first, then replace to avoid iterator invalidation)
    let recruit_matches: Vec<String> = RECRUIT_PATTERN.find_iter(&current_text).map(|m| m.as_str().to_string()).collect();
    for m in recruit_matches {
        mask_term(&m, &m, &mut current_text); // Replace with itself later
    }

    // 4. Custom Dictionary Terms (Sort by length descending to match longest terms first)
    let mut dict_entries: Vec<(&String, &String)> = custom_dict.iter().collect();
    dict_entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    for (ja, ko) in dict_entries {
        if current_text.contains(ja) {
            mask_term(ja, ko, &mut current_text);
        }
    }

    // 5. Numeric Units
    current_text = NUM_PATTERN_1.replace_all(&current_text, |caps: &Captures| {
        let placeholder = format!("[P{}]", p_count);
        replacements.insert(placeholder.clone(), format!("{}종", &caps[1]));
        p_count += 1;
        placeholder
    }).to_string();

    current_text = NUM_PATTERN_2.replace_all(&current_text, |caps: &Captures| {
        let placeholder = format!("[P{}]", p_count);
        replacements.insert(placeholder.clone(), format!("{}인", &caps[1]));
        p_count += 1;
        placeholder
    }).to_string();

    current_text = NUM_PATTERN_3.replace_all(&current_text, |caps: &Captures| {
        let placeholder = format!("[P{}]", p_count);
        replacements.insert(placeholder.clone(), format!("{}주", &caps[1]));
        p_count += 1;
        placeholder
    }).to_string();

    current_text = NUM_PATTERN_4.replace_all(&current_text, |caps: &Captures| {
        let placeholder = format!("[P{}]", p_count);
        replacements.insert(placeholder.clone(), format!("{}회", &caps[1]));
        p_count += 1;
        placeholder
    }).to_string();

    ShieldData { masked_text: current_text, replacements }
}

// --- POSTPROCESSOR ---
pub fn postprocess_text(translated: &str, shield: &ShieldData) -> String {
    // Strip out the think tags first!
    let mut final_text = THINK_PATTERN.replace_all(translated, "").to_string();

    // --- NEW: Safe Replacement Logic ---
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
    let space_punct = Regex::new(r"\s+([.!?,~])").unwrap();
    final_text = space_punct.replace_all(&final_text, "$1").to_string();

    // Collapse extra spaces
    let extra_spaces = Regex::new(r"\s+").unwrap();
    extra_spaces.replace_all(&final_text, " ").trim().to_string()
}

// --- NATIVE RUST JOSA FIXER ---
fn has_batchim(c: char) -> bool {
    let u = c as u32;
    // Check if character is within Hangul Syllables block
    if (0xAC00..=0xD7A3).contains(&u) {
        let code = u - 0xAC00;
        return (code % 28) != 0; // True if it has a final consonant
    }
    // Fallback for English/Numbers (rough approximation based on your python code)
    if "013678lmnLMN".contains(c) { return true; }
    false
}

pub fn load_dictionary(path: &Path) -> HashMap<String, String> {
    let mut custom_dict = HashMap::new();

    // 1. Read the file
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            println!("[Dictionary] Failed to read dict file: {}", e);
            // Returns an empty HashMap so the app doesn't crash, it just translates without the dict
            return custom_dict;
        }
    };

    if content.trim().is_empty() {
        println!("[Dictionary] Dict file is empty.");
        return custom_dict;
    }

    // 2. Parse the JSON safely
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(j) => j,
        Err(e) => {
            println!("[Dictionary] JSON Syntax Error: {}", e);
            return custom_dict;
        }
    };

    // 3. Extract the "data" object and map it
    if let Some(data) = json.get("data").and_then(|d| d.as_object()) {
        let ignored_brackets = "【】「」『』（）〈〉《》";

        for (k, v) in data {
            // Mimic Python's `if k not in "【】..."`
            if !ignored_brackets.contains(k) {
                if let Some(val_str) = v.as_str() {
                    custom_dict.insert(k.clone(), val_str.to_string());
                }
            }
        }
        println!("[Dictionary] Successfully loaded {} terms.", custom_dict.len());
    } else {
        println!("[Dictionary] Warning: 'data' key not found in custom_dict.json");
    }

    custom_dict
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

    #[test]
    fn test_batchim_detection() {
        // Test Hangul with batchim (final consonant)
        assert_eq!(has_batchim('각'), true);
        assert_eq!(has_batchim('은'), true);

        // Test Hangul without batchim
        assert_eq!(has_batchim('가'), false);
        assert_eq!(has_batchim('는'), false);

        // Test fallback numeric/english rules
        assert_eq!(has_batchim('1'), true);
        assert_eq!(has_batchim('2'), false);
        assert_eq!(has_batchim('L'), true);
    }

    #[test]
    fn test_shielding_pipeline() {
        let mut custom_dict = HashMap::new();
        custom_dict.insert("火力".to_string(), "딜러".to_string());
        custom_dict.insert("完凸".to_string(), "풀돌".to_string());

        let original_text = "【火力】@azururu 完凸 3周 <think>LLM is thinking...</think>";

        // 1. Test Preprocessor
        let shield = preprocess_text(original_text, &custom_dict, Some("Azururu"), Some("azururu"));

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
        assert_eq!(final_result, "【딜러】@azururu 풀돌 3주");
    }
}