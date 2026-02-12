import os
import sys
import json
import io
import argparse
import re
import ctranslate2
import sentencepiece as spm

# Force UTF-8 for stable pipe communication
sys.stdin = io.TextIOWrapper(sys.stdin.buffer, encoding='utf-8')
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

class TranslationManager:
    def __init__(self, dict_path):
        self.dict_path = dict_path
        # No punctuation here; we want the AI to see the sentence flow
        self.preserve_symbols = ["【", "】", "「", "」", "『", "』", "（", "）", "★", "◆", "〆"]
        self.custom_dict = {}
        self.load_dictionary()

    def log_info(self, msg):
        print(json.dumps({"type": "info", "message": msg}), flush=True)

    def log_error(self, msg):
        print(json.dumps({"type": "error", "message": msg}), flush=True)

    def load_dictionary(self):
        if os.path.exists(self.dict_path):
            try:
                with open(self.dict_path, 'r', encoding='utf-8') as f:
                    full_json = json.load(f)
                self.custom_dict = full_json.get("data", {})
                self.log_info(f"Dict Loaded: {len(self.custom_dict)} terms.")
            except Exception as e:
                self.log_error(f"Dict Error: {e}")

    def preprocess(self, text):
        placeholders = {}
        tag_count = 0
        diagnostics = []
        def add_diag(step, content): diagnostics.append({"step": step, "content": content})

        current_text = text
        add_diag("Original", current_text)

        # PHASE 0: NUMERIC PATTERN SHIELDING
        # Step 1: Convert Kanji counters to Korean (3種 -> 3종)
        current_text = re.sub(r'(\d+)種', r'\1종', current_text)
        current_text = re.sub(r'(\d+)人', r'\1인', current_text)
        current_text = re.sub(r'(\d+)周', r'\1회', current_text)

        # Step 2: IMMEDIATELY SHIELD the converted Korean counters
        # We look for "Digits + 종/인/바퀴" and hide them behind Z tags.
        # This prevents "3종" from becoming "세 가지 종류"
        counter_regex = r'\d+(?:종|인|바퀴)'
        for match in re.findall(counter_regex, current_text):
            tag = f"Z{tag_count}"
            placeholders[tag] = match  # We store "3종" as the hidden value
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1

        # PHASE 1: DICTIONARY
        if self.custom_dict:
            sorted_dict = sorted(self.custom_dict.items(), key=lambda x: len(x[0]), reverse=True)
            for ja, ko in sorted_dict:
                if ja in "～！？。 ": continue

                # FIX: Use word boundaries or check if the match is partial
                # For now, we ensure we don't match common short verbs like 'あり'
                # unless they are stand-alone or specific enough in your Gist.
                if ja in current_text:
                    tag = f"Z{tag_count}"
                    placeholders[tag] = ko
                    current_text = current_text.replace(ja, f" {tag} ")
                    tag_count += 1
        add_diag("Dictionary Shield", current_text)

        # PHASE 2: RECRUITMENT (@h, @T1D1)
        rec_regex = r'[＠@](?:[TtDdHh][0-9]|[a-zA-Z0-9]+)'
        for match in re.findall(rec_regex, current_text):
            tag = f"Z{tag_count}"
            placeholders[tag] = match
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1
        add_diag("Recruitment Shield", current_text)

        # PHASE 3: KAOMOJI
        kao_regex = r'\([^)]*[\*\'\"\^._\-;:/\\ω][^)]*\)'
        for match in re.findall(kao_regex, current_text):
            tag = f"Z{tag_count}"
            placeholders[tag] = match
            current_text = current_text.replace(match, f" {tag} ")
            tag_count += 1
        add_diag("Kaomoji Shield", current_text)

        # PHASE 4: SYMBOLS
        for sym in self.preserve_symbols:
            if sym in current_text:
                tag = f"Z{tag_count}"
                placeholders[tag] = sym
                current_text = current_text.replace(sym, f" {tag} ")
                tag_count += 1
        add_diag("Symbol Shield", current_text)

        return current_text, placeholders, diagnostics

    def postprocess(self, text, placeholders):
        final_text = text
        for tag in sorted(placeholders.keys(), key=len, reverse=True):
            pattern = re.compile(r'\s*' + re.escape(tag) + r'\s*')
            final_text = pattern.sub(f" {placeholders[tag]} ", final_text)
        return " ".join(final_text.split()).strip()

def get_optimized_hardware():
    cuda_count = ctranslate2.get_cuda_device_count()
    if cuda_count > 0:
        device = "cuda"
        supported = ctranslate2.get_supported_compute_types("cuda")
        # float16 for RTX 30/40; int8 for GTX 10-series
        compute_type = "float16" if "float16" in supported else "int8"
    else:
        device, compute_type = "cpu", "int8"
    return device, compute_type

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--dict", type=str, default="custom_dict.json")
    args = parser.parse_args()
    manager = TranslationManager(args.dict)

    try:
        device, compute_type = get_optimized_hardware()
        # Scale quality by hardware
        beam_size = 5 if compute_type == "float16" else 3

        translator = ctranslate2.Translator(args.model, device=device, compute_type=compute_type)
        sp = spm.SentencePieceProcessor(model_file=os.path.join(args.model, "tokenizer.model"))
        manager.log_info(f"AI Loaded: {device.upper()} ({compute_type}) Beam:{beam_size}")

        while True:
            line = sys.stdin.readline()
            if not line: break
            if not line.strip(): continue

            try:
                data = json.loads(line)
                if data.get("cmd") == "reload":
                    manager.load_dictionary()
                    continue

                input_text, pid = data.get("text", ""), data.get("pid")
                if pid is None: continue

                # 1. SHIELDING
                shielded_text, placeholders, diag = manager.preprocess(input_text)

                # 2. ADAPTIVE SEGMENTATION
                # Split by punctuation OR multiple spaces if the string is long
                # This ensures long recruitment posts are broken down properly.
                if any(p in shielded_text for p in "。！？\n"):
                    segments = re.split(r'([。！？\n])', shielded_text)
                else:
                    # Fallback: Split by spaces if no punctuation exists
                    segments = re.split(r'(\s{1,})', shielded_text)

                # Re-stitching segments with their delimiters
                combined_segments = []
                temp = ""
                for i in range(len(segments)):
                    temp += segments[i]
                    # If it's a delimiter or the end of the list, push it
                    if i % 2 != 0 or i == len(segments) - 1:
                        if temp.strip():
                            combined_segments.append(temp)
                        temp = ""

                # 3. BATCH INFERENCE
                batch_tokens = []
                for seg in combined_segments:
                    tokens = ["jpn_Jpan"] + sp.encode(seg, out_type=str) + ["</s>"]
                    batch_tokens.append(tokens)

                results = translator.translate_batch(
                    batch_tokens,
                    target_prefix=[["kor_Hang"]] * len(batch_tokens),
                    beam_size=beam_size,
                    repetition_penalty=1.1,
                    max_decoding_length=128
                )

                # 4. REASSEMBLY & CLEANUP
                translated_parts = []
                for res in results:
                    seg_out = sp.decode(res.hypotheses[0])
                    seg_out = re.sub(r'^[a-z]{3}_[A-Z][a-z]{3}\s*', '', seg_out).strip()
                    translated_parts.append(seg_out)

                raw_translated = " ".join(translated_parts)

                # 5. RESTORATION
                final_output = manager.postprocess(raw_translated, placeholders)

                print(json.dumps({
                    "type": "result", "pid": pid, "translated": final_output, "diagnostics": diag
                }, ensure_ascii=False), flush=True)

            except Exception as e:
                manager.log_error(f"Inference Error: {e}")
    except Exception as e:
        manager.log_error(f"Fatal Startup Error: {e}")

if __name__ == "__main__":
    main()