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
    Corrects Korean particles (을/를, 이/가, 은/는, 와/과)
    only when they are standalone or at the end of a word block.
    """
    def has_batchim(char):
        if not ('가' <= char <= '힣'): return False
        code = ord(char) - 44032
        return (code % 28) != 0

    # Refined Regex:
    # Group 1: The preceding word
    # Group 2: The particle
    # (?! [가-힣]): Negative lookahead - ensures the next char is NOT a Korean syllable
    pattern = re.compile(r'([가-힣a-zA-Z0-9\)]+)(을|를|이|가|은|는|와|과)(?![가-힣])')

    def replace_callback(match):
        word = match.group(1)
        particle = match.group(2)
        last_char = word[-1]

        # Determine batchim status of the last character
        has_final = has_batchim(last_char) if '가' <= last_char <= '힣' else False

        # Particle Mapping
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
    def preprocess_chunking(self, text):
        diagnostics = []
        diagnostics.append({"step": "1. Original Input", "content": text})

        split_marker = "||SPLIT||"
        current_text = text

        # Collect all strings that must be protected
        protected_map = {} # temp_id -> original_val
        protected_count = 0

        # A. Dictionary & Recruitment shielding
        all_targets = []
        if self.custom_dict:
            for ja, ko in self.custom_dict.items():
                if ja in current_text: all_targets.append((ja, ko))

        recruit_pattern = r'@[A-Za-z0-9]+(?:[\s]+[A-Za-z0-9]+)*'
        matches = re.findall(recruit_pattern, current_text)
        for m in set(matches): all_targets.append((m, m))

        all_targets.sort(key=lambda x: len(x[0]), reverse=True)

        # B. Replace targets with Split markers + unique ID
        for ja, target_val in all_targets:
            # We must use a unique ID to ensure we don't accidentally split
            # the same word multiple times if it appears twice
            placeholder = f"__PROTECTED_{protected_count}__"
            protected_map[placeholder] = target_val
            current_text = current_text.replace(ja, f"{split_marker}{placeholder}{split_marker}")
            protected_count += 1

        # C. Handle Numeric regexes
        num_patterns = [r'(\d+)種', r'(\d+)人', r'(\d+)周', r'(\d+)回']
        for p in num_patterns:
            def num_sub(m):
                nonlocal protected_count
                unit = "종" if "種" in m.group(0) else "인" if "人" in m.group(0) else "회"
                val = f"{m.group(1)}{unit}"
                placeholder = f"__PROTECTED_{protected_count}__"
                protected_map[placeholder] = val
                protected_count += 1
                return f"{split_marker}{placeholder}{split_marker}"
            current_text = re.sub(p, num_sub, current_text)

        # D. Split and Map back
        raw_chunks = [c.strip() for c in current_text.split(split_marker) if c.strip()]
        final_chunks = []
        for c in raw_chunks:
            if c in protected_map:
                final_chunks.append((protected_map[c], True)) # (text, is_protected)
            else:
                final_chunks.append((c, False))

        diagnostics.append({"step": "2. Chunked Segments", "content": [c[0] for c in final_chunks]})
        return final_chunks, diagnostics

    def preprocess(self, text):
        placeholders = {}
        tag_count = 0
        diagnostics = []

        def add_diag(step, content):
            diagnostics.append({"step": step, "content": content})

        current_text = text
        add_diag("1. Original Input", current_text)

        # We don't shield with tags anymore; we insert delimiters
        # to split the sentence into a list of parts.

        # PHASE 1: DICTIONARY
        if self.custom_dict:
            sorted_dict = sorted(self.custom_dict.items(), key=lambda x: len(x[0]), reverse=True)
            for ja, ko in sorted_dict:
                if ja in "～！？。♪☆★": continue
                if ja in current_text:
                    # Mark the dictionary term with a unique split marker
                    current_text = current_text.replace(ja, f"|||{ko}|||")

        # PHASE 2: NUMERIC PATTERNS
        current_text = re.sub(r'(\d+)種', r'||\1종||', current_text)
        current_text = re.sub(r'(\d+)人', r'||\1인||', current_text)
        current_text = re.sub(r'(\d+)周', r'||\1회||', current_text)
        current_text = re.sub(r'(\d+)回', r'||\1회||', current_text)

        # PHASE 3: RECRUITMENT
        recruit_pattern = r'@[A-Za-z0-9]+(?:[\s]+[A-Za-z0-9]+)*'
        matches = re.findall(recruit_pattern, current_text)
        for match in sorted(set(matches), key=len, reverse=True):
            current_text = current_text.replace(match, f"|||{match}|||")

        add_diag("2. Delimited Text", current_text)
        return current_text, placeholders, diagnostics

    def postprocess(self, text, placeholders):
        final_text = text

        for tag in sorted(placeholders.keys(), key=len, reverse=True):
            digit_match = re.search(r'\d+', tag)
            num = digit_match.group()
            data = placeholders[tag]
            target_value = data["val"]

            pattern = re.compile(r'[\(\[\\\{]?\s?TERM[\s_]*' + num + r'\s?[\)\]\\\}]?', re.IGNORECASE)

            if pattern.search(final_text):
                final_text = pattern.sub(f" {target_value} ", final_text)
            else:
                # RECOVERY MODE
                if data["is_start"]:
                    final_text = f"{target_value} {final_text}"
                elif data["anchor"] and data["anchor"] in final_text:
                    # Insert immediately after the anchor word
                    anchor_pattern = re.escape(data["anchor"])
                    final_text = re.sub(f"({anchor_pattern})", r"\1 " + target_value, final_text, count=1)
                else:
                    # Fallback to end if anchor is also gone
                    final_text = f"{final_text.strip()} {target_value}"

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

                # 1. PREPROCESS
                chunks_data, diagnostics = manager.preprocess_chunking(input_text)

                translated_parts = []
                chunk_details = []

                for idx, (chunk_text, is_protected) in enumerate(chunks_data):
                    if is_protected:
                        # DO NOT SEND TO AI
                        translated_parts.append(chunk_text)
                        chunk_details.append(f"Chunk {idx} [LOCKED]: {chunk_text}")
                    else:
                        # SEND TO AI
                        tokens = ["jpn_Jpan"] + sp.encode(chunk_text, out_type=str) + ["</s>"]
                        res = translator.translate_batch(
                            [tokens],
                            target_prefix=[["kor_Hang"]],
                            beam_size=3
                        )
                        seg_out = sp.decode(res[0].hypotheses[0])
                        seg_out = re.sub(r'^[a-z]{3}_[A-Z][a-z]{3}\s*', '', seg_out).strip()
                        translated_parts.append(seg_out)
                        chunk_details.append(f"Chunk {idx} [AI]: {chunk_text} -> {seg_out}")

                diagnostics.append({"step": "3. Translation Details", "content": chunk_details})

                # 2. REASSEMBLE
                # 1. Join chunks with a single space
                final_output = " ".join(filter(None, translated_parts))

                # 2. Clean up punctuation spacing (e.g., "word ." -> "word.")
                # This ensures particles followed by punctuation are still caught by the fixer
                final_output = re.sub(r'\s+([.!?,~])', r'\1', final_output)

                # 3. Apply the PROTECTED Josa fix
                final_output = fix_korean_josa(final_output)

                # 4. Final normalization of spaces
                final_output = " ".join(final_output.split()).strip()

                diagnostics.append({"step": "4. Final Polish", "content": final_output})

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