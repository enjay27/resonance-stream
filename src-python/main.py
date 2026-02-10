import sys
import argparse
import signal
import json
import os

# You need to install this: pip install llama-cpp-python
try:
    from llama_cpp import Llama
except ImportError:
    print(json.dumps({"error": "Missing llama-cpp-python module"}))
    sys.exit(1)

def signal_handler(sig, frame):
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)

def main():
    # 1. Parse Arguments (Passed from Rust)
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True, help="Path to .gguf file")
    parser.add_argument("--gpu_layers", type=int, default=-1, help="Number of GPU layers (-1 = all)")
    args = parser.parse_args()

    # 2. Check File
    if not os.path.exists(args.model):
        print(json.dumps({"type": "error", "message": f"Model file not found: {args.model}"}), flush=True)
        return

    # 3. Load Model
    print(json.dumps({"type": "status", "message": "Loading AI Engine..."}), flush=True)

    try:
        llm = Llama(
            model_path=args.model,
            n_gpu_layers=args.gpu_layers, # -1 for max GPU
            n_ctx=2048,                   # Context Window
            verbose=False                 # Reduce C++ logs
        )
        print(json.dumps({"type": "status", "message": "AI Ready"}), flush=True)
    except Exception as e:
        print(json.dumps({"type": "error", "message": str(e)}), flush=True)
        return

    # 4. Translation Loop (Read from Rust)
    # Rust will send: {"text": "こんにちは", "target": "KO"}
    for line in sys.stdin:
        try:
            line = line.strip()
            if not line: continue

            data = json.loads(line)
            input_text = data.get("text", "")

            if not input_text: continue

            # Construct Prompt (Qwen Chat Format)
            prompt = f"""<|im_start|>system
You are a translator. Translate the following Japanese text to Korean accurately. Output ONLY the Korean translation.<|im_end|>
<|im_start|>user
{input_text}<|im_end|>
<|im_start|>assistant
"""

            # Run Inference
            output = llm(
                prompt,
                max_tokens=256,
                stop=["<|im_end|>"],
                echo=False
            )

            translated_text = output['choices'][0]['text'].strip()

            # Send back to Rust
            response = {
                "type": "result",
                "original": input_text,
                "translated": translated_text
            }
            print(json.dumps(response), flush=True)

        except json.JSONDecodeError:
            pass # Ignore junk lines
        except Exception as e:
            print(json.dumps({"type": "error", "message": str(e)}), flush=True)

if __name__ == "__main__":
    main()