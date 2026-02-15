import argparse
import collections
import gc
import io
import json
import os
import re
import sys
import time

import ctranslate2
import pykakasi
import sentencepiece as spm

# Force UTF-8 for stable pipe communication
sys.stdin = io.TextIOWrapper(sys.stdin.buffer, encoding='utf-8')
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

# --- PRONUNCIATION SETUP ---
kks = pykakasi.kakasi()

# --- JOSA (PARTICLE) FIXER ---
def fix_korean_josa(text):
    def has_batchim(char):
        if not ('가' <= char <= '힣'): return False
        code = ord(char) - 44032
        return (code % 28) != 0

    pattern = re.compile(r'([가-힣a-zA-Z0-9\)]+)(을|를|이|가|은|는|와|과)(?![가-힣])')

    def replace_callback(match):
        word, particle = match.group(1), match.group(2)
        has_final = has_batchim(word[-1]) if '가' <= word[-1] <= '힣' else False
        mapping = {
            '을': '을' if has_final else '를', '를': '을' if has_final else '를',
            '이': '이' if has_final else '가', '가': '이' if has_final else '가',
            '은': '은' if has_final else '는', '는': '은' if has_final else '는',
            '와': '과' if has_final else '와', '과': '과' if has_final else '와'
        }
        return f"{word}{mapping.get(particle, particle)}"
    return pattern.sub(replace_callback, text)

# --- STATEFUL NICKNAME MANAGER ---
class NicknameManager:
    def __init__(self, limit=500):
        self.limit = limit
        self.nick_map = collections.OrderedDict()

    def update(self, jp_name, romaji):
        """FIFO logic to store and prioritize nicknames."""
        if not jp_name: return
        if jp_name in self.nick_map:
            self.nick_map.move_to_end(jp_name)
        self.nick_map[jp_name] = romaji
        if len(self.nick_map) > self.limit:
            self.nick_map.popitem(last=False)

    def get_map(self):
        return self.nick_map

# --- TRANSLATION MANAGER ---
class TranslationManager:
    def __init__(self, dict_path):
        self.dict_path = dict_path
        self.custom_dict = {}
        self.load_dictionary()

    def log_info(self, msg): print(json.dumps({"type": "info", "message": msg}), flush=True)
    def log_error(self, msg): print(json.dumps({"type": "error", "message": msg}), flush=True)

    def load_dictionary(self):
        self.log_info(f"Attempting to load dict from: {self.dict_path}")
        if os.path.exists(self.dict_path):
            try:
                with open(self.dict_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    if not content.strip():
                        self.log_error("Dict file is empty.")
                        return
                    raw_dict = json.loads(content).get("data", {})
                self.custom_dict = {k: v for k, v in raw_dict.items() if k not in "【】「」『』（）〈〉《》"}
                # RESTORED: Confirms dictionary status to Rust UI
                self.log_info(f"Dict Loaded: {len(self.custom_dict)} terms.")
            except Exception as e: self.log_error(f"Dict Error: {e}")
            except json.JSONDecodeError as je:
                self.log_error(f"Dict JSON Syntax Error: {je}") # 문법 오류 지점 표시

    def get_romaji(self, text):
        result = kks.convert(text)
        return "".join([item['hepburn'].capitalize() for item in result])

    def preprocess_chunking(self, text, stateful_nicks):
        original_input = text
        current_text = text

        # 1. Neutralize known nicknames
        for jp_name, romaji in stateful_nicks.items():
            if jp_name in current_text:
                current_text = current_text.replace(jp_name, romaji)

        split_marker, protected_map, protected_count = "||SPLIT||", {}, 0
        all_targets = []

        # 2. Recruitment & @-Tag
        recruit_pattern = r'@[A-Za-z0-9\u3040-\u30ff\u4e00-\u9faf]+(?:[\s]+[A-Za-z0-9\u3040-\u30ff\u4e00-\u9faf]+)*'
        for m in set(re.findall(recruit_pattern, current_text)):
            all_targets.append((m, m))

        # 3. Dictionary Terms
        for ja, ko in self.custom_dict.items():
            if ja in current_text:
                all_targets.append((ja, ko))

        all_targets.sort(key=lambda x: len(x[0]), reverse=True)
        for ja, replacement in all_targets:
            placeholder = f"__PTD_{protected_count}__"
            protected_map[placeholder] = replacement
            current_text = current_text.replace(ja, f"{split_marker}{placeholder}{split_marker}")
            protected_count += 1

        # 4. Numeric Units
        num_patterns = [r'(\d+)種', r'(\d+)人', r'(\d+)周', r'(\d+)回']
        for p in num_patterns:
            def num_sub(m):
                nonlocal protected_count
                unit = "종" if "種" in m.group(0) else "인" if "人" in m.group(0) else "회"
                val, placeholder = f"{m.group(1)}{unit}", f"__PTD_{protected_count}__"
                protected_map[placeholder] = val
                protected_count += 1
                return f"{split_marker}{placeholder}{split_marker}"
            current_text = re.sub(p, num_sub, current_text)

        raw_chunks = [c.strip() for c in current_text.split(split_marker) if c.strip()]
        final_chunks = [(protected_map[c], True) if c in protected_map else (c, False) for c in raw_chunks]
        return final_chunks, original_input

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--dict", type=str, default="custom_dict.json")
    parser.add_argument("--tier", type=str, choices=["low", "middle", "high", "extreme"], default="middle")
    parser.add_argument("--device", type=str, choices=["cpu", "cuda"], default="cuda")
    parser.add_argument("--debug", action="store_true", help="Enable diagnostics")
    args = parser.parse_args()

    manager = TranslationManager(args.dict)
    nick_manager = NicknameManager(limit=500)

    manager.log_info(f"Python Executable: {sys.executable}")
    manager.log_info(f"Python Version: {sys.version}")
    manager.log_info(f"Current Working Dir: {os.getcwd()}")

    try:
        cuda_count = ctranslate2.get_cuda_device_count()
        manager.log_info(f"GPU Device: {cuda_count} found.")
    except Exception:
        pass

    tier_cfg = {
        "low": {"beam": 1, "patience": 1.0, "rep_pen": 1.0},
        "middle": {"beam": 5, "patience": 1.0, "rep_pen": 1.1},
        "high": {"beam": 10, "patience": 2.0, "rep_pen": 1.1},
        "extreme": {"beam": 10, "patience": 2.0, "rep_pen": 1.3, "no_repeat": 3}
    }
    cfg = tier_cfg[args.tier]

    try:
        device, compute_type = ("cpu", "int8") if args.device == "cpu" else ("cuda", "int8_float16")
        if args.device == "cuda":
            cuda_count = ctranslate2.get_cuda_device_count()
            device, compute_type = ("cuda", "int8_float16") if cuda_count > 0 else ("cpu", "int8")
        if args.tier == "extreme":
            compute_type = "float16"

        model_files = ["model.bin", "config.json", "shared_vocabulary.json", "tokenizer.model"]
        for f in model_files:
            f_path = os.path.join(args.model, f)
            if not os.path.exists(f_path):
                manager.log_error(f"Missing critical model file: {f}")

        translator = ctranslate2.Translator(args.model, device=device, compute_type=compute_type, inter_threads=1, intra_threads=4)
        sp = spm.SentencePieceProcessor(model_file=os.path.join(args.model, "tokenizer.model"))
        manager.log_info(f"AI Started: {device.upper()} | Tier: {args.tier.upper()} | Debug: {args.debug}")

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
                raw_nickname = data.get("nickname")

                if data.get("cmd") == "nickname_only":
                    romaji = manager.get_romaji(raw_nickname)
                    print(json.dumps({
                        "type": "result",
                        "pid": pid,
                        "nickname": raw_nickname,
                        "romaji": romaji
                    }, ensure_ascii=False), flush=True)
                    continue

                if pid is None or not input_text: continue

                romaji = None
                if raw_nickname:
                    romaji = manager.get_romaji(raw_nickname)
                    nick_manager.update(raw_nickname, romaji)

                # Captures chunks and original input correctly for diagnostics
                chunks_data, original_input = manager.preprocess_chunking(input_text, nick_manager.get_map())
                translated_parts, chunk_details = [], []

                start_t = time.perf_counter()

                for idx, (chunk_text, is_protected) in enumerate(chunks_data):
                    if is_protected:
                        translated_parts.append(chunk_text)
                        if args.debug: chunk_details.append(f"Chunk {idx} [LOCKED]: {chunk_text}")
                    else:
                        tokens = ["jpn_Jpan"] + sp.encode(chunk_text, out_type=str) + ["</s>"]
                        res = translator.translate_batch(
                            [tokens],
                            target_prefix=[["kor_Hang"]],
                            beam_size=cfg["beam"],
                            patience=cfg["patience"],
                            repetition_penalty=cfg["rep_pen"],
                            no_repeat_ngram_size=cfg.get("no_repeat", 0),
                            max_batch_size=1,
                            batch_type="tokens"
                        )
                        seg_out = sp.decode(res[0].hypotheses[0])
                        seg_out = re.sub(r'^[a-z]{3}_[A-Z][a-z]{3}\s*', '', seg_out).strip()
                        translated_parts.append(seg_out)
                        if args.debug: chunk_details.append(f"Chunk {idx} [AI]: {chunk_text} -> {seg_out}")

                # Final assembly and polishing
                final_output = " ".join(filter(None, translated_parts))
                final_output = re.sub(r'\s+([.!?,~])', r'\1', final_output)
                final_output = fix_korean_josa(final_output)
                final_output = " ".join(final_output.split()).strip()

                result = {"type": "result", "pid": pid, "translated": final_output, "nickname": f"{raw_nickname}({romaji})"}
                if args.debug:
                    result["diagnostics"] = [
                        {"step": "1. Original", "content": original_input},
                        {"step": "2. Segments", "content": [c[0] for c in chunks_data]},
                        {"step": "3. Breakdown", "content": chunk_details},
                        {"step": "4. Final", "content": final_output}
                    ]

                end_t = time.perf_counter()
                latency_ms = (end_t - start_t) * 1000

                if args.debug:
                    manager.log_info(f"[Perf] Inference Time: {latency_ms:.2f}ms | Length: {len(input_text)} chars")

                print(json.dumps(result, ensure_ascii=False), flush=True)
                del chunks_data; gc.collect()

            except Exception as e: manager.log_error(f"Inference Error: {e}")
    except Exception as e: manager.log_error(f"Fatal Startup Error: {e}")

if __name__ == "__main__":
    main()