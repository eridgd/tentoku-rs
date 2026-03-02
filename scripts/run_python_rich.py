#!/usr/bin/env python3
"""
Tokenize a text file with the Python tentoku library and emit full rich output
(senses, glosses, POS, misc, field, dial, deinflection_reasons) as JSON.
Used for cross-validation with the Rust tentoku tokenizer.
"""
import argparse
import json
from pathlib import Path
import sys


def _prefer_local_reference():
    """Prefer local reference/tentoku over any globally installed package."""
    repo_root = Path(__file__).resolve().parents[1]
    reference_dir = repo_root / "reference"
    if (reference_dir / "tentoku").exists():
        sys.path.insert(0, str(reference_dir))


def token_to_comparable(token):
    """Convert a tentoku token to a JSON-serializable dict for comparison with Rust output."""
    out = {
        "text": getattr(token, "text", ""),
        "start": getattr(token, "start", 0),
        "end": getattr(token, "end", 0),
    }
    # Deinflection: list of chains, each chain is list of reason names
    reasons = getattr(token, "deinflection_reasons", None)
    if reasons is not None and len(reasons) > 0:
        out["deinflection_reasons"] = [
            [getattr(r, "name", str(r)) for r in chain] for chain in reasons
        ]
    else:
        out["deinflection_reasons"] = None

    entry = getattr(token, "dictionary_entry", None)
    if entry is None:
        out["dictionary_entry"] = None
        return out

    # Normalize ent_seq to string (Python may give int)
    ent_seq = getattr(entry, "ent_seq", None)
    out["dictionary_entry"] = {
        "ent_seq": str(ent_seq) if ent_seq is not None else "",
        "entry_id": getattr(entry, "entry_id", None),
        "kanji_readings": _readings(getattr(entry, "kanji_readings", [])),
        "kana_readings": _readings(getattr(entry, "kana_readings", [])),
        "senses": _senses(getattr(entry, "senses", [])),
    }
    return out


def _readings(readings):
    """Convert kanji/kana readings to comparable list of dicts."""
    result = []
    for r in readings or []:
        result.append({
            "text": getattr(r, "text", ""),
            "priority": getattr(r, "priority", None),
            "info": getattr(r, "info", None),
            "no_kanji": getattr(r, "no_kanji", None),
            "matched": getattr(r, "matched", None),
        })
    return result


def _senses(senses):
    """Convert senses to comparable list of dicts (glosses, pos_tags, misc, field, dial)."""
    result = []
    for i, s in enumerate(senses or []):
        glosses = []
        for g in getattr(s, "glosses", []) or []:
            glosses.append({
                "text": getattr(g, "text", ""),
                "lang": getattr(g, "lang", "en"),
                "g_type": getattr(g, "g_type", None),
            })
        result.append({
            "index": getattr(s, "index", i),
            "pos_tags": list(getattr(s, "pos_tags", []) or []),
            "glosses": glosses,
            "info": getattr(s, "info", None),
            "field": list(getattr(s, "field", []) or []) or None,
            "misc": list(getattr(s, "misc", []) or []) or None,
            "dial": list(getattr(s, "dial", []) or []) or None,
        })
    return result


def main():
    ap = argparse.ArgumentParser(description="Tokenize text with Python tentoku, output rich JSON")
    ap.add_argument("input_file", nargs="?", help="Path to text file (default: stdin)")
    ap.add_argument("--db", "-d", help="Path to JMDict SQLite DB (optional; default = tentoku default)")
    ap.add_argument("-o", "--output", help="Write JSON to file (default: stdout)")
    args = ap.parse_args()

    if args.input_file:
        with open(args.input_file, "r", encoding="utf-8") as f:
            text = f.read()
    else:
        text = sys.stdin.read()

    _prefer_local_reference()

    try:
        from tentoku.tokenizer import tokenize as py_tokenize
        from tentoku.sqlite_dict_optimized import OptimizedSQLiteDictionary

        if args.db:
            dictionary = OptimizedSQLiteDictionary(db_path=args.db, auto_build=False)
            tokens = py_tokenize(text, dictionary)
            dictionary.close()
        else:
            tokens = py_tokenize(text)
    except Exception as e:
        sys.stderr.write(f"tentoku error: {e}\n")
        sys.exit(1)

    out = [token_to_comparable(t) for t in tokens]
    json_str = json.dumps(out, ensure_ascii=False, indent=2)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(json_str)
    else:
        print(json_str)


if __name__ == "__main__":
    main()
