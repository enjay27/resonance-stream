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
    static ref RECRUIT_PATTERN: Regex = Regex::new(r"@[A-Za-z0-9\u3040-\u30ff\u4e00-\u9faf]+(?:[\s]+[A-Za-z0-9\u3040-\u30ff\u4e00-\u9faf]+)*").unwrap();
    static ref NUM_PATTERN_1: Regex = Regex::new(r"(\d+)種").unwrap();
    static ref NUM_PATTERN_2: Regex = Regex::new(r"(\d+)人").unwrap();
    static ref NUM_PATTERN_3: Regex = Regex::new(r"(\d+)周").unwrap();
    static ref NUM_PATTERN_4: Regex = Regex::new(r"(\d+)回").unwrap();
    static ref JOSA_PATTERN: Regex = Regex::new(r"([가-힣a-zA-Z0-9\)])(을|를|이|가|은|는|와|과)([^가-힣]|$)").unwrap();
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

    // 1. Replace Nickname if matched
    if let (Some(romaji), Some(ja_name)) = (nickname_romaji, original_nickname) {
        if current_text.contains(ja_name) {
            current_text = current_text.replace(ja_name, romaji);
        }
    }

    // Helper closure to mask terms
    let mut mask_term = |target: &str, replacement: &str, text: &mut String| {
        let placeholder = format!("[P{}]", p_count);
        *text = text.replace(target, &placeholder);
        replacements.insert(placeholder, replacement.to_string());
        p_count += 1;
    };

    // 2. Recruitment & @-Tag (Find all matches first, then replace to avoid iterator invalidation)
    let recruit_matches: Vec<String> = RECRUIT_PATTERN.find_iter(&current_text).map(|m| m.as_str().to_string()).collect();
    for m in recruit_matches {
        mask_term(&m, &m, &mut current_text); // Replace with itself later
    }

    // 3. Custom Dictionary Terms (Sort by length descending to match longest terms first)
    let mut dict_entries: Vec<(&String, &String)> = custom_dict.iter().collect();
    dict_entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    for (ja, ko) in dict_entries {
        if current_text.contains(ja) {
            mask_term(ja, ko, &mut current_text);
        }
    }

    // 4. Numeric Units
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

    // Restore shielded words
    for (placeholder, replacement) in &shield.replacements {
        final_text = final_text.replace(placeholder, replacement);
    }

    // Clean up weird LLM spacing around punctuation
    let space_punct = Regex::new(r"\s+([.!?,~])").unwrap();
    final_text = space_punct.replace_all(&final_text, "$1").to_string();

    // Fix Korean Josa (Particles)
    final_text = fix_korean_josa(&final_text);

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

fn fix_korean_josa(text: &str) -> String {
    JOSA_PATTERN.replace_all(text, |caps: &Captures| {
        let word = &caps[1];
        let particle = &caps[2];
        let trailing = &caps[3]; // Crucial: Capture the space or punctuation!

        // Get the last character of the word
        let last_char = word.chars().last().unwrap_or(' ');
        let final_cons = has_batchim(last_char);

        let fixed_particle = match particle {
            "을" | "를" => if final_cons { "을" } else { "를" },
            "이" | "가" => if final_cons { "이" } else { "가" },
            "은" | "는" => if final_cons { "은" } else { "는" },
            "와" | "과" => if final_cons { "과" } else { "와" },
            _ => particle,
        };

        // Recombine: Word + Corrected Particle + Trailing character
        format!("{}{}{}", word, fixed_particle, trailing)
    }).to_string()
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