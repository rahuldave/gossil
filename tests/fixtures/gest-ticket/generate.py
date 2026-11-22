"""Spell the Gest ticket creation fixtures independently of the Rust emitter.

Each card is written by hand from the field mapping of GEST-01 and the card
order of Fossil tkt.c:1837-1874 (b3a1c8efb2731dffbd5cc38c58ac349746de6da6).
The Z card is the lowercase MD5 of the preceding bytes, from Python hashlib.
"""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def fossilize(text):
    """Escape a token as Fossil's defossilize expects; JSON text has no raw controls."""
    out = []
    for ch in text:
        out.append({"\\": "\\\\", " ": "\\s", "\n": "\\n", "\r": "\\r", "\t": "\\t",
                    "\f": "\\f", "\v": "\\v", "\0": "\\0"}.get(ch, ch))
    return "".join(out)


def artifact(date, fields, kind, ticket, actor):
    """Return creation bytes with model JSON fields, the three fixed fields and a checksum."""
    cards = {name: fossilize(json.dumps(value, ensure_ascii=False, separators=(",", ":")))
             for name, value in fields.items()}
    cards["gossil_deleted"] = "false"
    cards["gossil_kind"] = kind
    cards["gossil_schema"] = "1"
    text = f"D {date}\n"
    for name in sorted(cards):
        text += f"J {name} {cards[name]}\n"
    text += f"K {ticket}\nU {fossilize(actor)}\n"
    body = text.encode()
    return body + b"Z " + hashlib.md5(body).hexdigest().encode() + b"\n"


def main():
    """Write the artifact, project and both task creation fixtures."""
    (ROOT.parent / "task-ticket" / "creation.artifact").write_bytes(artifact(
        "2024-02-29T12:34:56.000000001",
        {
            "gest_id": "k" * 32,
            "project_id": "z" * 32,
            "title": "hello",
            "priority": None,
            "status": "in-progress",
            "description": "",
            "assigned_to": None,
            "metadata": None,
            "resolved_at": None,
            "created_at": "2024-01-01T00:00:00+00:00",
            "updated_at": "2024-01-01T00:00:00+00:00",
        },
        "task", "0123456789abcdef0123456789abcdef01234567", "native actor"))
    (ROOT / "artifact-creation.artifact").write_bytes(artifact(
        "2026-11-20T01:02:03.000000004",
        {
            "gest_id": "zyxwvutsrqponmlkzyxwvutsrqponmlk",
            "project_id": "k" * 32,
            "title": "Plan é",
            "body": "Step 1\n\tC:\\path",
            "metadata": {"a": [None, False], "b": "x y"},
            "archived_at": None,
            "created_at": "2026-11-18T12:00:00+00:00",
            "updated_at": "2026-11-18T12:30:00+00:00",
        },
        "artifact", "fedcba9876543210fedcba9876543210fedcba98", "gest agent"))
    (ROOT / "project-creation.artifact").write_bytes(artifact(
        "2026-11-20T01:02:03.000000004",
        {
            "gest_id": "klmnopqrstuvwxyzklmnopqrstuvwxyz",
            "archived_at": "2026-11-19T00:00:00+00:00",
            "created_at": "2026-11-18T12:00:00+00:00",
            "updated_at": "2026-11-18T12:30:00+00:00",
        },
        "project", "0123456789abcdef0123456789abcdef01234567", "gest agent"))
    (ROOT / "task-creation-full.artifact").write_bytes(artifact(
        "2024-03-01T00:00:00.000000000",
        {
            "gest_id": "k" * 32,
            "project_id": "z" * 32,
            "title": "Fix é bug",
            "priority": 3,
            "status": "done",
            "description": "a\\b",
            "assigned_to": "l" * 32,
            "metadata": {"labels": ["x"], "n": 1},
            "resolved_at": "2024-02-01T00:00:00+00:00",
            "created_at": "2024-01-01T00:00:00+00:00",
            "updated_at": "2024-02-01T00:00:00+00:00",
        },
        "task", "0123456789abcdef0123456789abcdef01234567", "native actor"))


if __name__ == "__main__":
    main()
