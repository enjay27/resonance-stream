import os
import sys
import json
import io
import argparse
import re  # Added: Essential for regex shielding
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

    def log_status(self, msg):
        print(json.dumps({"type": "status", "message": msg}), flush=True)

    def log_info(self, msg):
        print(json.dumps({"type": "info", "message": msg}), flush=True)

    def log_error(self, msg):
        print(json.dumps({"type": "error", "message": msg}), flush=True)

    def load_dictionary(self):
        if os.path.exists(self.dict_path):
            try:
                with open(self.dict_path, 'r', encoding='utf-8') as f:
                    self.custom_dict = json.load(f)
                self.log_info(f"Dictionary loaded: {len(self.custom_dict)} terms.")
            except Exception as e:
                self.log_error(f"Failed to load dictionary: {e}")

    def preprocess(self, text):
        placeholders = {}
        tag_count = 0
        diagnostics = []

        # Helper to log the state after each step
        def add_diagnostic(step_name, current_text):
            diagnostics.append({"step": step_name, "content": current_text})

        current_text = text
        add_diagnostic("Original", current_text)

        # PHASE 1: Complex Recruitment (e.g., @T1D1H1)
        # Using [＠@] to handle both half-width and full-width symbols
        complex_regex = r'[＠@](?:[TtDdHh][0-9])+'
        matches = re.findall(complex_regex, current_text)
        for match in matches:
            tag = f"PROT_RECRUIT_{tag_count}"
            placeholders[tag] = match
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1
        add_diagnostic("Recruitment Shield", current_text)

        # PHASE 2: Kaomoji/Emoticons (e.g., (*'ω'*))
        # Protects patterns with internal punctuation often mangled by AI
        kaomoji_regex = r'\([^)]*[\*\'\"\^._\-;:/\\ω][^)]*\)'
        matches = re.findall(kaomoji_regex, current_text)
        for match in matches:
            tag = f"PROT_KAO_{tag_count}"
            placeholders[tag] = match
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1
        add_diagnostic("Kaomoji Shield", current_text)

        # PHASE 3: Custom Dictionary (From Gist)
        # Sorted by length to ensure "イ마진" is caught before "イ"
        sorted_dict = sorted(self.custom_dict.items(), key=lambda x: len(x[0]), reverse=True)
        for ja, ko in sorted_dict:
            if ja in current_text:
                tag = f"PROT_DICT_{tag_count}"
                placeholders[tag] = ko # Shielding with the KOREAN target value
                current_text = current_text.replace(ja, f" {tag} ")
                tag_count += 1
        add_diagnostic("Dictionary Shield", current_text)

        # PHASE 4: Individual Symbols (～, 【, etc.)
        for sym in self.preserve_symbols:
            if sym in current_text:
                tag = f"PROT_SYM_{tag_count}"
                placeholders[tag] = sym
                current_text = current_text.replace(sym, f" {tag} ")
                tag_count += 1
        add_diagnostic("Symbol Shield", current_text)

        return current_text, placeholders, diagnostics

    def postprocess(self, text, placeholders):
        final_text = text
        for tag, val in placeholders.items():
            # Handle cases where AI adds spaces around the tag
            final_text = final_text.replace(f" {tag} ", val)
            final_text = final_text.replace(tag, val)
        return final_text

def get_optimized_hardware():
    cuda_count = ctranslate2.get_cuda_device_count()
    if cuda_count > 0:
        device = "cuda"
        supported = ctranslate2.get_supported_compute_types("cuda")
        # float16 is best for RTX 4080 Super
        if "float16" in supported:
            compute_type = "float16"
        else:
            compute_type = "int8"
    else:
        device = "cpu"
        compute_type = "int8"
    return device, compute_type

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--dict", type=str, default="custom_dict.json")
    args = parser.parse_args()

    manager = TranslationManager(args.dict)
    manager.log_status("Initializing AI Engine...")

    try:
        device, compute_type = get_optimized_hardware()
        manager.log_info(f"Hardware: {device.upper()} ({compute_type})")

        translator = ctranslate2.Translator(
            args.model,
            device=device,
            compute_type=compute_type,
            inter_threads=1,
            intra_threads=1 # Safer for concurrent game usage
        )

        sp = spm.SentencePieceProcessor(model_file=os.path.join(args.model, "tokenizer.model"))
        manager.log_status("NLLB Ready")

        while True:
            line = sys.stdin.readline()
            if not line: break

            try:
                data = json.loads(line.strip())
                if data.get("cmd") == "reload":
                    manager.load_dictionary()
                    manager.log_info("Dictionary reloaded.")
                    continue

                input_text = data.get("text", "")
                packet_pid = data.get("pid")
                if packet_pid is None: continue

                # 1. PREPROCESS
                clean_text, placeholders, debug_steps = manager.preprocess(input_text)

                print(json.dumps({
                    "type": "diagnostic",
                    "pid": packet_pid,
                    "steps": debug_steps
                }, ensure_ascii=False), flush=True)

                # 2. INFERENCE
                source_tokens = ["jpn_Jpan"] + sp.encode(clean_text, out_type=str) + ["</s>"]
                results = translator.translate_batch(
                    [source_tokens],
                    target_prefix=[["kor_Hang"]],
                    beam_size=1
                )

                # 3. POSTPROCESS
                translated_tokens = results[0].hypotheses[0]
                if "kor_Hang" in translated_tokens: translated_tokens.remove("kor_Hang")

                raw_translated = sp.decode(translated_tokens)
                final_output = manager.postprocess(raw_translated, placeholders)

                print(json.dumps({
                    "type": "result",
                    "pid": packet_pid,
                    "translated": final_output
                }, ensure_ascii=False), flush=True)

            except Exception as e:
                manager.log_error(f"Inference Loop Error: {e}")

    except Exception as e:
        manager.log_error(f"Fatal Startup Error: {e}")

if __name__ == "__main__":
    main()