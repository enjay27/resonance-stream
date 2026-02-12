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

def main():
    parser = argparse.ArgumentParser()
    # model_path now points to the folder containing model.bin, config.json, etc.
    parser.add_argument("--model", type=str, required=True)
    parser.add_argument("--device", type=str, default="cpu", choices=["cpu", "cuda", "auto"])
    args = parser.parse_args()

    print(json.dumps({"type": "status", "message": "NLLB Ready"}), flush=True)

    try:
        # 1. Initialize the Translator
        # compute_type="int8" ensures it runs fast on common CPUs
        translator = ctranslate2.Translator(
            args.model,
            device=args.device,
            compute_type="int8" if args.device == "cpu" else "default"
        )

        # 2. Initialize the SentencePiece Tokenizer
        # The model manager downloads this to the same directory
        sp_path = os.path.join(args.model, "tokenizer.model")
        if not os.path.exists(sp_path):
            # Diagnostic for you in the console
            print(json.dumps({"type": "error", "message": f"File not found: {sp_path}"}))

        sp = spm.SentencePieceProcessor(model_file=sp_path)

        print(json.dumps({"type": "status", "message": "NLLB Lite Engine Ready"}), flush=True)
    except Exception as e:
        print(json.dumps({"type": "error", "message": f"Init failed: {str(e)}"}), flush=True)
        return

    # 3. Processing Loop
    while True:
        line = sys.stdin.readline()
        if not line: break

        try:
            data = json.loads(line.strip())
            input_text = data.get("text", "")
            packet_pid = data.get("pid")

            # FIX: Use out_type=str to send actual tokens, not IDs
            # NLLB requires the source language tag (e.g., jpn_Jpan)
            source_tokens = ["jpn_Jpan"] + sp.encode(input_text, out_type=str) + ["</s>"]

            # Perform Translation
            # target_prefix MUST also be a list of string tokens
            results = translator.translate_batch(
                [source_tokens],
                target_prefix=[["kor_Hang"]],
                beam_size=2,
                batch_type="tokens",
                max_batch_size=1024
            )

            # Extract results
            translated_tokens = results[0].hypotheses[0]

            # Remove the target language tag if present
            if "kor_Hang" in translated_tokens:
                translated_tokens.remove("kor_Hang")

            final_output = sp.decode(translated_tokens)

            # Echo the result back to app.rs
            print(json.dumps({
                "type": "result",
                "pid": packet_pid,
                "original": input_text,
                "translated": final_output
            }, ensure_ascii=False), flush=True)

        except Exception as e:
            print(json.dumps({"type": "error", "message": str(e)}), flush=True)

if __name__ == "__main__":
    main()