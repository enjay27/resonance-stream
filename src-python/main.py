import argparse
import collections
import io
import json
import os
import re
import sys
import time
import gc

import ctranslate2
import pykakasi
from transformers import AutoTokenizer

# --- Post-Processing Libraries ---
import hanja
from kyujipy import KyujitaiConverter

# Force UTF-8 for stable pipe communication
sys.stdin = io.TextIOWrapper(sys.stdin.buffer, encoding='utf-8')
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

# --- GLOBALLY PRE-COMPILED REGEXES ---
JOSA_PATTERN = re.compile(r'([가-힣a-zA-Z0-9\)]+)(을|를|이|가|은|는|와|과)(?![가-힣])')
RECRUIT_PATTERN = re.compile(r'@[A-Za-z0-9\u3040-\u30ff\u4e00-\u9faf]+(?:[\s]+[A-Za-z0-9\u3040-\u30ff\u4e00-\u9faf]+)*')
NUM_PATTERNS = [
    (re.compile(r'(\d+)種'), '종'),
    (re.compile(r'(\d+)人'), '인'),
    (re.compile(r'(\d+)周'), '회'),
    (re.compile(r'(\d+)回'), '회')
]

kks = pykakasi.kakasi()

def fix_korean_josa(text):
    def has_batchim(char):
        if not ('가' <= char <= '힣'): return False
        return ((ord(char) - 44032) % 28) != 0

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

    return JOSA_PATTERN.sub(replace_callback, text)

class NicknameManager:
    def __init__(self, limit=500):
        self.limit = limit
        self.nick_map = collections.OrderedDict()

    def update(self, jp_name, romaji):
        """FIFO logic restored from previous version"""
        if not jp_name: return
        if jp_name in self.nick_map:
            self.nick_map.move_to_end(jp_name)
        self.nick_map[jp_name] = romaji
        if len(self.nick_map) > self.limit:
            self.nick_map.popitem(last=False)

    def get_map(self):
        return self.nick_map

class TranslationManager:
    def __init__(self, dict_path, model_path=None):
        self.dict_path = dict_path
        self.model_path = model_path
        self.custom_dict = {}
        self.dict_pattern = None
        self.kanji_converter = KyujitaiConverter()
        self.load_dictionary()

    def log_info(self, msg): print(json.dumps({"type": "info", "message": msg}), flush=True)
    def log_error(self, msg): print(json.dumps({"type": "error", "message": msg}), flush=True)
    def log_debug(self, msg): print(json.dumps({"type": "debug", "message": msg}), flush=True)

    def load_dictionary(self):
        target_path = self.dict_path
        if not os.path.exists(target_path) and self.model_path:
            app_data_root = os.path.abspath(os.path.join(self.model_path, "../../"))
            fallback_path = os.path.join(app_data_root, "custom_dict.json")
            if os.path.exists(fallback_path): target_path = fallback_path

        self.log_info(f"Loading dictionary from: {target_path}")
        if os.path.exists(target_path):
            try:
                with open(target_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    if not content.strip(): return
                    raw_dict = json.loads(content).get("data", {})
                self.custom_dict = {k: v for k, v in raw_dict.items() if k not in "【】「」『』（）〈〉《》"}
                if self.custom_dict:
                    sorted_keys = sorted(self.custom_dict.keys(), key=len, reverse=True)
                    self.dict_pattern = re.compile(f"({'|'.join([re.escape(k) for k in sorted_keys])})")
                    self.log_info(f"Dictionary ready: {len(self.custom_dict)} terms vectorized.")
            except Exception as e: self.log_error(f"Dictionary Load Error: {e}")

    def get_romaji(self, text):
        return "".join([item['hepburn'].capitalize() for item in kks.convert(text)])

    def preprocess_chunking(self, text, stateful_nicks):
        current_text = text.replace('、', ', ').replace('。', '. ').replace('・', ', ').replace('　', ' ')
        for jp_name, romaji in stateful_nicks.items():
            if jp_name in current_text: current_text = current_text.replace(jp_name, romaji)

        split_marker, protected_map, protected_count = "💠SPLIT💠", {}, 0
        def protect(val):
            nonlocal protected_count
            placeholder = f"💠{protected_count}💠"
            protected_map[placeholder] = val
            protected_count += 1
            return f"{split_marker}{placeholder}{split_marker}"

        current_text = RECRUIT_PATTERN.sub(lambda m: protect(m.group(0)), current_text)
        if self.dict_pattern: current_text = self.dict_pattern.sub(lambda m: protect(self.custom_dict[m.group(1)]), current_text)
        for pattern, unit in NUM_PATTERNS: current_text = pattern.sub(lambda m: protect(f"{m.group(1)}{unit}"), current_text)

        raw_chunks = [c.strip() for c in current_text.split(split_marker) if c.strip()]
        return [(protected_map[c], True) if c in protected_map else (c, False) for c in raw_chunks]

# --- REFACTORED INFERENCE CORE ---
def process_batch_inference(messages, manager, nick_manager, generator, tokenizer, cfg):
    chunk_mapping = []
    ai_prompts_tokens = []
    diagnostic_info = []

    sys_prompt = (
        "<|im_start|>system\n"
        "당신은 MMORPG 번역기입니다. 모르는 한자와 단축어(T, D, DPS 등)는 원본 그대로 유지하고, "
        "부연 설명 없이 한국어 번역만 출력하세요. /no_think<|im_end|>\n"
        "<|im_start|>user\n"
    )
    sys_tokens = tokenizer.convert_ids_to_tokens(tokenizer.encode(sys_prompt))
    end_tokens = tokenizer.convert_ids_to_tokens(tokenizer.encode("<|im_end|>\n<|im_start|>assistant\n"))

    for msg_idx, msg in enumerate(messages):
        nickname = msg.get("nickname")
        if nickname: nick_manager.update(nickname, manager.get_romaji(nickname))

        chunks_data = manager.preprocess_chunking(msg.get("text", ""), nick_manager.get_map())
        diagnostic_info.append(chunks_data)

        for chunk_idx, (chunk_text, is_protected) in enumerate(chunks_data):
            if is_protected or not re.search(r'[a-zA-Z\u3040-\u30ff\u4e00-\u9faf가-힣]', chunk_text):
                chunk_mapping.append((msg_idx, chunk_idx, True, chunk_text))
            else:
                tokens = sys_tokens + tokenizer.convert_ids_to_tokens(tokenizer.encode(chunk_text)) + end_tokens
                chunk_mapping.append((msg_idx, chunk_idx, False, len(ai_prompts_tokens)))
                ai_prompts_tokens.append(tokens)

    if ai_prompts_tokens:
        ai_results = generator.generate_batch(
            ai_prompts_tokens,
            beam_size=cfg["beam"],
            sampling_temperature=cfg["temp"],
            repetition_penalty=1.1,
            max_length=60,
            batch_type="tokens",
            include_prompt_in_result=False
        )

    results_map = collections.defaultdict(list)
    for msg_idx, chunk_idx, is_protected, payload in chunk_mapping:
        if is_protected:
            results_map[msg_idx].append((chunk_idx, payload))
        else:
            seg_out = tokenizer.decode(ai_results[payload].sequences_ids[0]).strip()
            seg_out = re.sub(r'<think>.*?</think>', '', seg_out, flags=re.DOTALL).strip()
            seg_out = manager.kanji_converter.shinjitai_to_kyujitai(seg_out)
            seg_out = hanja.translate(seg_out, 'substitution')
            results_map[msg_idx].append((chunk_idx, seg_out))

    final_results = []
    diag_outputs = []
    for msg_idx in range(len(messages)):
        sorted_chunks = sorted(results_map[msg_idx], key=lambda x: x[0])
        parts = [c[1] for c in sorted_chunks]
        txt = re.sub(r'\s+([.!?,~])', r'\1', " ".join(filter(None, parts)))
        final_txt = fix_korean_josa(txt).strip()

        final_results.append({"pid": messages[msg_idx].get("pid"), "translated": final_txt})

        breakdown = []
        for i, (orig_chunk, is_locked) in enumerate(diagnostic_info[msg_idx]):
            status = "[LOCKED]" if is_locked else "[AI]"
            breakdown.append(f"Chunk {i} {status}: {orig_chunk} -> {parts[i]}")

        diag_outputs.append({
            "pid": messages[msg_idx].get("pid"),
            "original": messages[msg_idx].get("text"),
            "breakdown": breakdown,
            "final": final_txt
        })

    return final_results, diag_outputs, len(ai_prompts_tokens)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--dict", type=str, default="custom_dict.json")
    parser.add_argument("--tier", default="middle")
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--debug", action="store_true", help="Performance and detailed logs")
    parser.add_argument("--diagnostics", action="store_true", help="Chunk breakdown logs")
    parser.add_argument("--version", type=str, default="0.0.0")
    args = parser.parse_args()

    manager = TranslationManager(args.dict, args.model)
    nick_manager = NicknameManager()
    cfg = {"beam": 2, "temp": 0.1}

    manager.log_info(f"Python: {sys.version.split()[0]} | Device: {args.device.upper()} | Model: {os.path.basename(args.model)}")
    compute_type = "float16" if "AWQ" in args.model else ("int8_float16" if args.device == "cuda" else "int8")

    try:
        generator = ctranslate2.Generator(args.model, device=args.device, compute_type=compute_type)
        tokenizer = AutoTokenizer.from_pretrained(args.model)
        print(json.dumps({"type": "ready"}), flush=True)
        manager.log_info(f"Engine Ready | Tier: {args.tier} | Compute: {compute_type} | Beam: {cfg['beam']}")

        while True:
            line = sys.stdin.readline()
            if not line or not line.strip(): continue
            try:
                data = json.loads(line)
                cmd = data.get("cmd")

                # --- RESTORED: NICKNAME ONLY COMMAND ---
                if cmd == "nickname_only":
                    raw_nickname = data.get("nickname")
                    romaji = manager.get_romaji(raw_nickname)
                    print(json.dumps({
                        "type": "result", "pid": data.get("pid"),
                        "nickname": raw_nickname, "romaji": romaji
                    }, ensure_ascii=False), flush=True)
                    continue

                if cmd in ["batch_translate"]:
                    msgs = data.get("messages")
                    if msgs is None: msgs = [data] if data.get("text") else []
                    if not msgs: continue

                    start_t = time.perf_counter()
                    results, diags, ai_count = process_batch_inference(msgs, manager, nick_manager, generator, tokenizer, cfg)

                    print(json.dumps({"type": "batch_result", "results": results}, ensure_ascii=False), flush=True)

                    if args.debug:
                        manager.log_debug(f"[Performance] Processed {len(msgs)} msgs ({ai_count} AI) in {(time.perf_counter() - start_t) * 1000:.2f}ms")
                    if args.diagnostics:
                        for d in diags: manager.log_debug(f"[Diagnostics PID {d['pid']}] {d}")

                    # --- RESTORED: MEMORY MANAGEMENT ---
                    del results; gc.collect()

                elif cmd == "reload": manager.load_dictionary()
            except Exception as e: manager.log_error(f"Inference Loop Error: {e}")
    except Exception as e: manager.log_error(f"Fatal Startup Error: {e}")

if __name__ == "__main__": main()