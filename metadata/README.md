# metadata/

What the app reads about the translation model and the custom dictionary, as **sources**. They are
not read by the app from here: the `metadata.yml` workflow (a pushed tag `metadata-v<N>`) turns them into
signed files on the generated `metadata` branch, and the app fetches and verifies those, so it can tell
them from anything an attacker could serve. See `docs/decisions.md` D-28.

| file | what | edited by |
|---|---|---|
| `metadata.json` | the model (version, URL, SHA-256, notes) and the dictionary's version and date | Kade |
| `custom_dict.json` | the custom dictionary: `{ "category": { "ja": "ko" } }` | Kade |

The published `metadata.json` also has a `revision` (the number in the tag `metadata-v<N>`) and the
dictionary's `sha256`, both added when it is signed; never write them here.

Publishing: change the files in a pull request, merge it, then push the tag `metadata-v<N>` (N one
higher than the last) on that commit of `main`. Nothing is published, or signed, by a merge alone.
