# .memory — which file takes what

`MEMORY.md` (repo root) is the index. It holds only what would be *false* the moment
it goes stale: what is next, and what is written but not yet verified. Everything
else lives here, one folder per kind of note.

| folder | takes | rule |
|---|---|---|
| `active-issues/` | things **not to trust yet**: code or docs written but never run on Windows, docs that contradict the code, known bugs | one file per topic; **keep each under ~150 lines** — close items by deleting them, the history is in `sessions/` and git |
| `roadmap/` | what is next, with the check that proves it done; facts with a number | one file per workstream |
| `sessions/` | dated write-ups `YYYY-MM-DD-<topic>.md`, **including wrong turns** | append-only; flat folder |

Write the note in the same commit as the change it describes.
