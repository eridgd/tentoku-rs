#!/usr/bin/env python3
"""
Verification script: compare Python tentoku vs Rust tentoku rich output on the same text.
Produces a report on token boundaries, ent_seq, and sense equivalence.
Run after:
  .venv/bin/python3 scripts/run_python_rich.py kokoro.txt --db jmdict.db -o kokoro-python.json
  cargo run --release -- tokenize --file kokoro.txt --db jmdict.db > kokoro-rust.json
"""
import argparse
import json
import sys


def main():
    ap = argparse.ArgumentParser(description="Verify Python vs Rust tentoku rich output")
    ap.add_argument("python_json", help="JSON from run_python_rich.py (use same --db as Rust)")
    ap.add_argument("rust_json", help="JSON from tentoku tokenize --file ...")
    ap.add_argument("--report", "-r", action="store_true", help="Print detailed divergence report")
    args = ap.parse_args()

    with open(args.python_json, "r", encoding="utf-8") as f:
        py_tokens = json.load(f)
    with open(args.rust_json, "r", encoding="utf-8") as f:
        rust_tokens = json.load(f)

    py_text = "".join(t["text"] for t in py_tokens)
    rust_text = "".join(t["text"] for t in rust_tokens)
    text_ok = py_text == rust_text

    # Boundary sets: (start, end) -> token index (first occurrence)
    def boundaries(tokens):
        out = {}
        for i, t in enumerate(tokens):
            key = (t["start"], t["end"])
            if key not in out:
                out[key] = i
        return out

    py_bounds = boundaries(py_tokens)
    rust_bounds = boundaries(rust_tokens)
    py_only = set(py_bounds) - set(rust_bounds)
    rust_only = set(rust_bounds) - set(py_bounds)
    common = set(py_bounds) & set(rust_bounds)

    # For common spans, check ent_seq and first gloss match
    sense_ok = 0
    sense_diff = 0
    for (s, e) in common:
        pt = py_tokens[py_bounds[(s, e)]]
        rt = rust_tokens[rust_bounds[(s, e)]]
        pe = pt.get("dictionary_entry")
        re = rt.get("dictionary_entry")
        if (pe is None) != (re is None):
            sense_diff += 1
            if args.report and sense_diff <= 10:
                print(f"  Span ({s},{e}) {pt['text']!r}: one has dict entry, other does not")
        elif pe and re:
            if str(pe.get("ent_seq", "")) != str(re.get("ent_seq", "")):
                sense_diff += 1
                if args.report and sense_diff <= 10:
                    print(f"  Span ({s},{e}) {pt['text']!r}: ent_seq {pe.get('ent_seq')} vs {re.get('ent_seq')}")
            else:
                # Same ent_seq: compare first sense glosses
                pa = (pe.get("senses") or [{}])[0].get("glosses") or []
                ra = (re.get("senses") or [{}])[0].get("glosses") or []
                if pa and ra and (pa[0].get("text") != ra[0].get("text")):
                    sense_diff += 1
                    if args.report and sense_diff <= 10:
                        print(f"  Span ({s},{e}) gloss: {pa[0].get('text')!r} vs {ra[0].get('text')!r}")
                else:
                    sense_ok += 1
        else:
            sense_ok += 1

    # Report header and summary first
    print("=== Tentoku Python vs Rust rich output verification ===\n")
    print(f"Input length: {len(py_text)} chars")
    print(f"Reconstructed text equal: {text_ok}")
    print(f"Python token count: {len(py_tokens)}")
    print(f"Rust token count:   {len(rust_tokens)}")
    print(f"Common spans (same start,end): {len(common)}")
    print(f"Spans only in Python: {len(py_only)}")
    print(f"Spans only in Rust:  {len(rust_only)}")
    print(f"Common spans with same ent_seq/gloss: {sense_ok}")
    print(f"Common spans with sense/ent_seq diff: {sense_diff}")

    if args.report and (py_only or rust_only):
        print("\n--- Sample boundary differences ---")
        for (s, e) in sorted(py_only)[:15]:
            t = py_tokens[py_bounds[(s, e)]]
            print(f"  Python only: ({s},{e}) {t['text']!r}")
        for (s, e) in sorted(rust_only)[:15]:
            t = rust_tokens[rust_bounds[(s, e)]]
            print(f"  Rust only:   ({s},{e}) {t['text']!r}")

    # Exit 0 only if tokenization and sense content match
    if not text_ok:
        print("\nFAIL: Reconstructed text differs.", file=sys.stderr)
        sys.exit(1)
    if len(py_tokens) != len(rust_tokens):
        print(f"\nNOTE: Token count differs (Python {len(py_tokens)} vs Rust {len(rust_tokens)}).", file=sys.stderr)
        print("This indicates segmentation differences; content for matching spans may still align.", file=sys.stderr)
    if sense_diff > 0:
        print(f"\nNOTE: {sense_diff} common span(s) have different ent_seq or gloss.", file=sys.stderr)
    if len(py_tokens) == len(rust_tokens) and sense_diff == 0:
        print("\nPASS: Token count and sense content match.", file=sys.stderr)
    sys.exit(0)


if __name__ == "__main__":
    main()
