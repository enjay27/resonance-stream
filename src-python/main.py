import os
import sys
import json
import io
import argparse
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
        # 1. Shield Symbols
        for i, sym in enumerate(self.preserve_symbols):
            if sym in text:
                tag = f"__S{i}__"
                placeholders[tag] = sym
                text = text.replace(sym, tag)
        # 2. Apply Dictionary Terms
        for i, (ja, ko) in enumerate(self.custom_dict.items()):
            if ja in text:
                tag = f"__D{i}__"
                placeholders[tag] = ko
                text = text.replace(ja, tag)
        return text, placeholders

    def postprocess(self, text, placeholders):
        for tag, val in placeholders.items():
            text = text.replace(tag, val)
        return text

def get_optimized_hardware():
    """Detects hardware and returns (device, compute_type)"""
    cuda_count = ctranslate2.get_cuda_device_count()
    if cuda_count > 0:
        device = "cuda"
        supported = ctranslate2.get_supported_compute_types("cuda")
        if "int8_float16" in supported:
            compute_type = "int8_float16"
        elif "float16" in supported:
            compute_type = "float16"
        else:
            compute_type = "int8"
    else:
        device = "cpu"
        compute_type = "int8" # Essential for CPU speed
    return device, compute_type

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--dict", type=str, default="custom_dict.json")
    args = parser.parse_args()

    manager = TranslationManager(args.dict)
    manager.log_status("Initializing AI Engine...")

    try:
        # 1. Hardware Auto-Detection
        device, compute_type = get_optimized_hardware()
        manager.log_info(f"Hardware Detected: {device.upper()} ({compute_type})")

        # 2. Load Translator & Tokenizer
        translator = ctranslate2.Translator(
            args.model,
            device=device,
            compute_type=compute_type,
            inter_threads=1,
            intra_threads=4
        )

        sp = spm.SentencePieceProcessor(model_file=os.path.join(args.model, "tokenizer.model"))
        manager.log_status("NLLB Ready")

        # 3. Processing Loop
        while True:
            line = sys.stdin.readline()
            if not line: break

            try:
                data = json.loads(line.strip())

                # Check for Dictionary Reload
                if data.get("cmd") == "reload":
                    manager.load_dictionary()
                    manager.log_info("Dictionary hot-reloaded.")
                    continue

                input_text = data.get("text", "")
                packet_pid = data.get("pid")

                # Robust PID check (allows PID 0)
                if packet_pid is None: continue

                # Pre-process
                clean_text, placeholders = manager.preprocess(input_text)

                # Translation
                source_tokens = ["jpn_Jpan"] + sp.encode(clean_text, out_type=str) + ["</s>"]
                results = translator.translate_batch(
                    [source_tokens],
                    target_prefix=[["kor_Hang"]],
                    beam_size=1
                )

                # Decode & Post-process
                translated_tokens = results[0].hypotheses[0]
                if "kor_Hang" in translated_tokens: translated_tokens.remove("kor_Hang")
                raw_translated = sp.decode(translated_tokens)
                final_output = manager.postprocess(raw_translated, placeholders)

                # Return to Rust
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