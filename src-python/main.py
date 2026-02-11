import sys
import argparse
import json
import os
import io
import re

# Force UTF-8 for reliable cross-platform pipe communication
sys.stdin = io.TextIOWrapper(sys.stdin.buffer, encoding='utf-8', errors='replace')
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')

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
            n_ctx=2048,
            verbose=False
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
            req_id = data.get("id", None) # Capture the Unique ID

            if not input_text: continue

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
            # Clean up thinking tags if the model uses them
            clean_text = re.sub(r'<think>.*?</think>', '', raw_text, flags=re.DOTALL).strip()

            response = {
                "type": "result",
                "id": req_id, # Echo the ID back to the UI
                "original": input_text,
                "translated": clean_text
            }
            print(json.dumps(response, ensure_ascii=False), flush=True)

        except Exception as e:
            print(json.dumps({"type": "error", "message": str(e)}), flush=True)

if __name__ == "__main__":
    main()