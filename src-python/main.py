import os
import sys
import json
import io
import argparse
import re
import ctranslate2
import sentencepiece as spm

# Force UTF-8 for stable pipe communication with Rust/Tauri
sys.stdin = io.TextIOWrapper(sys.stdin.buffer, encoding='utf-8')
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

class TranslationManager:
    def __init__(self, dict_path):
        self.dict_path = dict_path
        self.preserve_symbols = [
            "【", "】", "「", "」", "『", "』", "（", "）",
            "〈", "〉", "《", "》", "～", "・", "★", "◆", "〆", "！", "？"
        ]
        self.custom_dict = {}
        self.load_dictionary()

    def log_info(self, msg):
        print(json.dumps({"type": "info", "message": msg}), flush=True)

    def log_error(self, msg):
        print(json.dumps({"type": "error", "message": msg}), flush=True)

    def load_dictionary(self):
        """Handles nested Gist structure: {"version": "...", "data": {...}}"""
        if os.path.exists(self.dict_path):
            try:
                with open(self.dict_path, 'r', encoding='utf-8') as f:
                    full_json = json.load(f)

                # Correctly target the nested "data" key
                self.custom_dict = full_json.get("data", {})
                version = full_json.get("version", "unknown")

                self.log_info(f"Dictionary v{version} loaded: {len(self.custom_dict)} terms.")
            except Exception as e:
                self.log_error(f"Failed to load dictionary: {e}")

    def preprocess(self, text):
        placeholders = {}
        tag_count = 0
        diagnostics = []

        def add_diagnostic(step_name, current_text):
            diagnostics.append({"step": step_name, "content": current_text})

        current_text = text
        add_diagnostic("Original", current_text)

        # Alphabetical seeds for indestructible tags (e.g., VXYZA)
        alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

        # PHASE 1: DICTIONARY PRIORITY (千夢 -> 꿈엮기)
        if self.custom_dict:
            # Sort by length descending to prevent partial matching
            sorted_dict = sorted(self.custom_dict.items(), key=lambda x: len(x[0]), reverse=True)
            for ja, ko in sorted_dict:
                if ja in current_text:
                    tag = f"VXYZ{alphabet[tag_count % 26]}"
                    placeholders[tag] = ko
                    current_text = current_text.replace(ja, f" {tag} ")
                    tag_count += 1
        add_diagnostic("Dictionary Shield", current_text)

        # PHASE 2: RECRUITMENT (@h, @T1D1H1)
        rec_regex = r'[＠@](?:[TtDdHh][0-9]|[a-zA-Z0-9]+)'
        matches = re.findall(rec_regex, current_text)
        for match in matches:
            tag = f"VXYZ{alphabet[tag_count % 26]}"
            placeholders[tag] = match
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1
        add_diagnostic("Recruitment Shield", current_text)

        # PHASE 3: KAOMOJI (Emoticons)
        kao_regex = r'\([^)]*[\*\'\"\^._\-;:/\\ω][^)]*\)'
        kao_matches = re.findall(kao_regex, current_text)
        for match in kao_matches:
            tag = f"VXYZ{alphabet[tag_count % 26]}"
            placeholders[tag] = match
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1
        add_diagnostic("Kaomoji Shield", current_text)

        # PHASE 4: SYMBOL FALLBACK
        for sym in self.preserve_symbols:
            if sym in current_text:
                tag = f"VXYZ{alphabet[tag_count % 26]}"
                placeholders[tag] = sym
                current_text = current_text.replace(sym, f" {tag} ")
                tag_count += 1
        add_diagnostic("Symbol Shield", current_text)

        return current_text, placeholders, diagnostics

    def postprocess(self, translated_text, placeholders):
        final_text = translated_text
        # Sort tags by length descending to prevent partial tag collision
        sorted_tags = sorted(placeholders.keys(), key=len, reverse=True)
        for tag in sorted_tags:
            val = placeholders[tag]
            # Use regex to clean up any spaces the AI added around tags
            pattern = re.compile(r'\s*' + re.escape(tag) + r'\s*')
            final_text = pattern.sub(f" {val} ", final_text)

        return " ".join(final_text.split()).strip()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--dict", type=str, default="custom_dict.json")
    args = parser.parse_args()

    manager = TranslationManager(args.dict)

    try:
        # Optimized for RTX 4080 Super
        translator = ctranslate2.Translator(args.model, device="cuda", compute_type="float16")
        sp = spm.SentencePieceProcessor(model_file=os.path.join(args.model, "tokenizer.model"))
        manager.log_info("AI Ready on GPU (Float16)")

        while True:
            line = sys.stdin.readline()
            if not line: break
            cleaned_line = line.strip()
            if not cleaned_line: continue

            try:
                data = json.loads(cleaned_line)
                if data.get("cmd") == "reload":
                    manager.load_dictionary()
                    continue

                input_text, packet_pid = data.get("text", ""), data.get("pid")
                if packet_pid is None: continue

                # 1. PREPROCESS
                clean_text, placeholders, diag = manager.preprocess(input_text)

                # 2. INFERENCE (Optimized Beam Size)
                source_tokens = ["jpn_Jpan"] + sp.encode(clean_text, out_type=str) + ["</s>"]
                results = translator.translate_batch([source_tokens], target_prefix=[["kor_Hang"]], beam_size=2)

                # 3. METADATA CLEANUP
                raw_translated = sp.decode(results[0].hypotheses[0])
                # Remove leaked language tags like 'kor_Hang'
                raw_translated = re.sub(r'^[a-z]{3}_[A-Z][a-z]{3}\s*', '', raw_translated).strip()

                # 4. POSTPROCESS
                final_output = manager.postprocess(raw_translated, placeholders)

                print(json.dumps({
                    "type": "result", "pid": packet_pid,
                    "translated": final_output, "diagnostics": diag
                }, ensure_ascii=False), flush=True)

            except Exception as e:
                manager.log_error(f"Loop error: {e}")

    except Exception as e:
        manager.log_error(f"Fatal crash: {e}")

if __name__ == "__main__":
    main()