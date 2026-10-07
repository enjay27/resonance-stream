#!/usr/bin/env python3
"""Prints the per-file SHA-256 table for crates/core/src/server_pins.rs.

Usage: python3 -I .github/scripts/ai-server-pins.py <llama-...-win-vulkan-x64.zip>

The zip is the one `AI_SERVER_ZIP_URL` points at (src-tauri/src/services/downloader/server.rs).
The app extracts it flat (folders dropped), so the names here are the flattened ones. Check
the zip's own SHA-256 against `AI_SERVER_ZIP_SHA256` first; this script prints it too.
Change the table, the zip URL and `AI_SERVER_ZIP_SHA256` together.
"""
import hashlib
import posixpath
import sys
import zipfile


def main(path):
    sha = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            sha.update(chunk)
    print(f"// zip sha256: {sha.hexdigest()}")
    rows = {}
    with zipfile.ZipFile(path) as z:
        for info in z.infolist():
            if info.filename.endswith("/"):
                continue
            name = posixpath.basename(info.filename)
            if name in rows:
                sys.exit(f"two entries flatten to {name}: the app would keep the last one")
            rows[name] = hashlib.sha256(z.read(info)).hexdigest()
    for name in sorted(rows):
        print(f'    Pin {{ name: "{name}", sha256: "{rows[name]}" }},')


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
