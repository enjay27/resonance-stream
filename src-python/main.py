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

# --- JOSA (PARTICLE) FIXER ---
def fix_korean_josa(text):
    """
    Corrects Korean particle mismatches (을/를, 이/가, 은/는, 와/과).
    """
    def has_batchim(char):
        if not ('가' <= char <= '힣'): return False
        code = ord(char) - 44032
        return (code % 28) != 0

    pattern = re.compile(r'([가-힣a-zA-Z0-9\)]+)\s*([을를이가은는와과])')

    def replace_callback(match):
        word = match.group(1)
        particle = match.group(2)
        last_char = word[-1]
        has_final = has_batchim(last_char) if '가' <= last_char <= '힣' else False

        if particle in ['을', '를']: new_p = '을' if has_final else '를'
        elif particle in ['이', '가']: new_p = '이' if has_final else '가'
        elif particle in ['은', '는']: new_p = '은' if has_final else '는'
        elif particle in ['와', '과']: new_p = '과' if has_final else '와'
        else: new_p = particle

        return f"{word}{new_p}"

    return pattern.sub(replace_callback, text)

# --- TRANSLATION MANAGER ---
class TranslationManager:
    def __init__(self, dict_path):
        self.dict_path = dict_path
        self.target_brackets = "【】「」『』（）〈〉《》＞＜≫≪«»”‘’“"
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
                raw_dict = full_json.get("data", {})
                self.custom_dict = {}
                for k, v in raw_dict.items():
                    if k in self.target_brackets: continue
                    self.custom_dict[k] = v
                self.log_info(f"Dict Loaded: {len(self.custom_dict)} terms.")
            except Exception as e:
                self.log_error(f"Dict Error: {e}")

    def preprocess(self, text):
        placeholders = {}
        tag_count = 0
        diagnostics = []

        # Helper to log steps inside preprocess
        def add_diag(step, content):
            diagnostics.append({"step": step, "content": content})

        current_text = text
        add_diag("1. Original Input", current_text)

        # PHASE 1: DICTIONARY
        if self.custom_dict:
            sorted_dict = sorted(self.custom_dict.items(), key=lambda x: len(x[0]), reverse=True)
            for ja, ko in sorted_dict:
                if ja in "～！？。♪☆★": continue
                if ja in current_text:
                    tag = f"[#{tag_count}]"
                    placeholders[tag] = ko
                    current_text = current_text.replace(ja, tag)
                    tag_count += 1

        # PHASE 2: NUMERIC PATTERNS
        current_text = re.sub(r'(\d+)種', r'\1종', current_text)
        current_text = re.sub(r'(\d+)人', r'\1인', current_text)
        current_text = re.sub(r'(\d+)周', r'\1회', current_text)
        current_text = re.sub(r'(\d+)回', r'\1회', current_text)

        # PHASE 3: RECRUITMENT & KAOMOJI
        # (Simplified for brevity - ensure your regexes are here)

        add_diag("2. Dictionary Shielded", current_text)
        return current_text, placeholders, diagnostics

    def postprocess(self, text, placeholders):
        final_text = text
        for tag in sorted(placeholders.keys(), key=len, reverse=True):
            num = tag.strip("[]#")
            pattern = re.compile(rf'\[\s*#\s*{num}\s*\]')
            final_text = pattern.sub(f" {placeholders[tag]} ", final_text)
        return " ".join(final_text.split()).strip()

def get_optimized_hardware():
    cuda_count = ctranslate2.get_cuda_device_count()
    if cuda_count > 0:
        device = "cuda"
        supported = ctranslate2.get_supported_compute_types("cuda")
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
        translator = ctranslate2.Translator(args.model, device=device, compute_type=compute_type)
        sp = spm.SentencePieceProcessor(model_file=os.path.join(args.model, "tokenizer.model"))
        manager.log_info(f"AI Loaded: {device.upper()} ({compute_type})")

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

                # ==========================================================
                # RECURSIVE BATCH TRANSLATION
                # ==========================================================

                # 1. PREPROCESS (Shielding)
                shielded_text, placeholders, diagnostics = manager.preprocess(input_text)

                # Helper to add to the main diagnostics list
                def add_diag(step, content):
                    diagnostics.append({"step": step, "content": content})

                # 2. EXTRACT BRACKETS
                bracket_pattern = r'([【「『（〈《＜≪«“‘])(.*?)([】」』）〉》＞≫»”’])'
                matches = re.findall(bracket_pattern, shielded_text)

                temp_main_text = shielded_text
                parts_to_translate = []

                for idx, (open_b, content, close_b) in enumerate(matches):
                    parts_to_translate.append(content)

                    # Wrap placeholder in original brackets to guide NLLB context
                    placeholder = f"{open_b}__ITEM_{idx}__{close_b}"
                    full_match_str = f"{open_b}{content}{close_b}"
                    temp_main_text = temp_main_text.replace(full_match_str, placeholder, 1)

                add_diag("3. Extracted Frame", temp_main_text)
                add_diag("3. Extracted Parts", parts_to_translate)

                # 3. PREPARE BATCH
                batch_inputs = [temp_main_text] + parts_to_translate

                # 4. TOKENIZE
                batch_tokens = []
                for txt in batch_inputs:
                    tokens = ["jpn_Jpan"] + sp.encode(txt, out_type=str) + ["</s>"]
                    batch_tokens.append(tokens)

                # 5. TRANSLATE
                results = translator.translate_batch(
                    batch_tokens,
                    target_prefix=[["kor_Hang"]] * len(batch_tokens),
                    beam_size=3,
                    repetition_penalty=1.1,
                    max_decoding_length=128
                )

                # 6. DECODE
                decoded_results = []
                for res in results:
                    seg_out = sp.decode(res.hypotheses[0])
                    seg_out = re.sub(r'^[a-z]{3}_[A-Z][a-z]{3}\s*', '', seg_out).strip()
                    decoded_results.append(seg_out)

                final_main = decoded_results[0]
                translated_inners = decoded_results[1:]

                add_diag("4. AI Output (Frame)", final_main)
                add_diag("4. AI Output (Parts)", translated_inners)

                # 7. REASSEMBLE
                for idx, inner_trans in enumerate(translated_inners):
                    open_b, _, close_b = matches[idx]
                    reassembled = f"{open_b}{inner_trans}{close_b}"

                    # ROBUST REGEX: Finds "ITEM_0", "__ITEM_0__", "ITEM 0", etc.
                    # Also handles if NLLB changed the surrounding brackets/quotes
                    punc_open = r'([\"\'「『【（〈《＜≪«“‘\s]*)'
                    punc_close = r'([\"\'」』】）〉》＞≫»”’\s]*)'
                    tag_core = r'(?:__|)?\s*ITEM\s*[_\s]*' + str(idx) + r'\s*(?:__|)?'

                    placeholder_regex = re.compile(punc_open + tag_core + punc_close)

                    # Verify if we actually found the tag before replacing
                    if placeholder_regex.search(final_main):
                        final_main = placeholder_regex.sub(reassembled, final_main)
                    else:
                        add_diag(f"Warning: Tag {idx} Lost", "AI hallucinations removed the tag")

                add_diag("5. Reassembled", final_main)

                # 8. UNSHIELD
                final_output = manager.postprocess(final_main, placeholders)

                # 9. FIX PARTICLES
                final_output = fix_korean_josa(final_output)
                add_diag("6. Final Polish", final_output)

                print(json.dumps({
                    "type": "result",
                    "pid": pid,
                    "translated": final_output,
                    "diagnostics": diagnostics
                }, ensure_ascii=False), flush=True)

            except Exception as e:
                manager.log_error(f"Inference Error: {e}")

    except Exception as e:
        manager.log_error(f"Fatal Startup Error: {e}")

if __name__ == "__main__":
    main()