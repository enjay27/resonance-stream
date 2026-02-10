import sys
import argparse
import signal
import json
import os
import io
import re  # NEW IMPORT for Regex

# Force UTF-8 (Fixes the encoding crash from before)
sys.stdin = io.TextIOWrapper(sys.stdin.buffer, encoding='utf-8', errors='replace')
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')

# ... (Keep existing imports/handlers) ...

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--gpu_layers", type=int, default=-1)
    args = parser.parse_args()

    if not os.path.exists(args.model):
        print(json.dumps({"type": "error", "message": "Model not found"}), flush=True)
        return

    print(json.dumps({"type": "status", "message": "Loading AI..."}), flush=True)

    try:
        from llama_cpp import Llama
        llm = Llama(
            model_path=args.model,
            n_gpu_layers=args.gpu_layers,
            n_ctx=2048,      # This causes the warning (but saves RAM)
            verbose=False    # This hides the warning from the logs!
        )
        print(json.dumps({"type": "status", "message": "AI Ready"}), flush=True)
    except Exception as e:
        print(json.dumps({"type": "error", "message": str(e)}), flush=True)
        return

    # Processing Loop
    for line in sys.stdin:
        try:
            line = line.strip()
            if not line: continue

            data = json.loads(line)
            input_text = data.get("text", "")
            if not input_text: continue

            # Prompt Template
            prompt = f"""<|im_start|>system
You are a translator. Translate the Japanese text to Korean. Output ONLY the translation.<|im_end|>
<|im_start|>user
{input_text}<|im_end|>
<|im_start|>assistant
"""

            output = llm(
                prompt,
                max_tokens=256,
                stop=["<|im_end|>"],
                echo=False
            )

            raw_text = output['choices'][0]['text']

            # --- CLEAN UP <think> TAGS ---
            # Remove everything between <think> and </think>
            clean_text = re.sub(r'<think>.*?</think>', '', raw_text, flags=re.DOTALL).strip()

            response = {
                "type": "result",
                "original": input_text,
                "translated": clean_text
            }
            print(json.dumps(response, ensure_ascii=False), flush=True)

        except Exception as e:
            print(json.dumps({"type": "error", "message": str(e)}), flush=True)

if __name__ == "__main__":
    main()