#!/usr/bin/env python3
"""
Compare rich tokenizer output from Python tentoku vs Rust tentoku.
Asserts that the same input produces semantically equivalent content:
same token count, same text/start/end, same ent_seq and sense content where applicable.
"""
import argparse
import json
import sys


def norm_list(v):
    """Normalize list or None to list for comparison."""
    if v is None:
        return []
    return list(v)


def norm_str(v):
    if v is None:
        return ""
    return str(v).strip()


def senses_equal(a, b):
    """Compare two sense lists (order matters)."""
    if len(a) != len(b):
        return False, f"sense count {len(a)} vs {len(b)}"
    for i, (sa, sb) in enumerate(zip(a, b)):
        if norm_list(sa.get("pos_tags")) != norm_list(sb.get("pos_tags")):
            return False, f"token sense[{i}] pos_tags differ"
        glosses_a = norm_list(sa.get("glosses"))
        glosses_b = norm_list(sb.get("glosses"))
        if len(glosses_a) != len(glosses_b):
            return False, f"token sense[{i}] gloss count {len(glosses_a)} vs {len(glosses_b)}"
        for j, (ga, gb) in enumerate(zip(glosses_a, glosses_b)):
            if norm_str(ga.get("text")) != norm_str(gb.get("text")):
                return False, f"token sense[{i}] gloss[{j}] text differ: {ga.get('text')!r} vs {gb.get('text')!r}"
        for key in ("field", "misc", "dial"):
            if norm_list(sa.get(key)) != norm_list(sb.get(key)):
                return False, f"token sense[{i}] {key} differ"
    return True, None


def deinflection_equal(ra, rb):
    """Compare deinflection_reasons (list of list of strings)."""
    na = norm_list(ra) if ra else []
    nb = norm_list(rb) if rb else []
    if len(na) != len(nb):
        return False, f"deinflection chain count {len(na)} vs {len(nb)}"
    for i, (ca, cb) in enumerate(zip(na, nb)):
        la = [norm_str(x) for x in (ca if isinstance(ca, list) else [ca])]
        lb = [norm_str(x) for x in (cb if isinstance(cb, list) else [cb])]
        if la != lb:
            return False, f"deinflection chain[{i}] {la} vs {lb}"
    return True, None


def token_equal(ta, tb, token_index):
    """Compare two token dicts. Returns (equal: bool, message)."""
    if norm_str(ta.get("text")) != norm_str(tb.get("text")):
        return False, f"token[{token_index}] text: {ta.get('text')!r} vs {tb.get('text')!r}"
    if int(ta.get("start", 0)) != int(tb.get("start", 0)):
        return False, f"token[{token_index}] start: {ta.get('start')} vs {tb.get('start')}"
    if int(ta.get("end", 0)) != int(tb.get("end", 0)):
        return False, f"token[{token_index}] end: {ta.get('end')} vs {tb.get('end')}"

    ea = ta.get("dictionary_entry")
    eb = tb.get("dictionary_entry")
    if (ea is None) != (eb is None):
        return False, f"token[{token_index}] one has dictionary_entry, other does not"
    if ea is not None and eb is not None:
        if norm_str(ea.get("ent_seq")) != norm_str(eb.get("ent_seq")):
            return False, f"token[{token_index}] ent_seq: {ea.get('ent_seq')} vs {eb.get('ent_seq')}"
        ok, msg = senses_equal(ea.get("senses", []), eb.get("senses", []))
        if not ok:
            return False, f"token[{token_index}] {msg}"

    ok, msg = deinflection_equal(ta.get("deinflection_reasons"), tb.get("deinflection_reasons"))
    if not ok:
        return False, f"token[{token_index}] {msg}"
    return True, None


def main():
    ap = argparse.ArgumentParser(description="Compare Python vs Rust tentoku rich JSON output")
    ap.add_argument("python_json", help="JSON file from run_python_rich.py")
    ap.add_argument("rust_json", help="JSON file from tentoku tokenize --file ...")
    ap.add_argument("--quiet", "-q", action="store_true", help="Only print summary")
    args = ap.parse_args()

    with open(args.python_json, "r", encoding="utf-8") as f:
        py_tokens = json.load(f)
    with open(args.rust_json, "r", encoding="utf-8") as f:
        rust_tokens = json.load(f)

    if len(py_tokens) != len(rust_tokens):
        print(f"MISMATCH: token count Python={len(py_tokens)} Rust={len(rust_tokens)}", file=sys.stderr)
        sys.exit(1)

    errors = []
    for i, (pt, rt) in enumerate(zip(py_tokens, rust_tokens)):
        ok, msg = token_equal(pt, rt, i)
        if not ok:
            errors.append((i, msg))

    if errors:
        for i, msg in errors[:20]:
            print(f"  [{i}] {msg}", file=sys.stderr)
        if len(errors) > 20:
            print(f"  ... and {len(errors) - 20} more", file=sys.stderr)
        print(f"FAIL: {len(errors)} token(s) differ", file=sys.stderr)
        sys.exit(1)

    if not args.quiet:
        print(f"OK: {len(py_tokens)} tokens match (Python vs Rust rich output).")
    sys.exit(0)


if __name__ == "__main__":
    main()
