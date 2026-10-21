# SPDX-License-Identifier: AGPL-3.0-or-later
"""RUST SOURCE AS THE COMPILER SEES IT, for `engine_id` (build_site_app.py).

A file is read as its tokens: comments, doc comments and layout are gone, every
literal is kept byte for byte, and an item under `#[cfg(test)]` is dropped with
the files its `mod x;` names. An edit that leaves the tokens alone cannot move
a number or a gas count, so it moves no engine.

Errs one way only: a construct this does not understand is kept as tokens.

    python3 scripts/rust_tokens.py --self-test
"""
import json
import sys
from pathlib import Path

WORD = lambda c: c.isalnum() or c == "_"
TEST_ATTR = ["#", "[", "cfg", "(", "test", ")", "]"]


def _string(s: str, i: int) -> int:
    """Index past a `"…"` literal whose opening quote is at `i`."""
    i += 1
    while i < len(s) and s[i] != '"':
        i += 2 if s[i] == "\\" else 1
    return i + 1


def lex(s: str) -> list:
    out, i, n = [], 0, len(s)
    while i < n:
        c = s[i]
        if c.isspace():
            i += 1
        elif s.startswith("//", i):
            j = s.find("\n", i)
            i = n if j < 0 else j
        elif s.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if s.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif s.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif c == '"':
            j = _string(s, i)
            out.append(s[i:j])
            i = j
        elif c == "'":
            # A char literal ('a', '\n', '\u{1F600}') or a lifetime ('a, 'static).
            if i + 1 < n and s[i + 1] == "\\":
                j = i + 1
                while j < n and s[j] != "'":
                    j += 2 if s[j] == "\\" else 1
                out.append(s[i:j + 1])
                i = j + 1
            elif i + 2 < n and s[i + 2] == "'":
                out.append(s[i:i + 3])
                i += 3
            else:
                j = i + 1
                while j < n and WORD(s[j]):
                    j += 1
                out.append(s[i:j])
                i = j
        elif WORD(c):
            j = i
            while j < n and WORD(s[j]):
                j += 1
            word = s[i:j]
            k = j
            while k < n and s[k] == "#":
                k += 1
            if word in ("r", "br", "cr") and k < n and s[k] == '"':
                close = '"' + "#" * (k - j)
                end = s.find(close, k + 1)
                end = n if end < 0 else end + len(close)
                out.append(s[i:end])
                i = end
            elif word in ("b", "c") and j < n and s[j] in "\"'":
                end = _string(s, j) if s[j] == '"' else s.find("'", j + 2 if s[j + 1] == "\\" else j + 1) + 1
                out.append(s[i:end])
                i = end
            else:
                out.append(word)
                i = j
        else:
            out.append(c)
            i += 1
    return out


def _past_attrs(t: list, i: int) -> int:
    """Index past the outer attributes starting at `i`."""
    while t[i:i + 2] == ["#", "["]:
        depth, i = 0, i + 1
        while i < len(t):
            depth += {"[": 1, "]": -1}.get(t[i], 0)
            i += 1
            if depth == 0:
                break
    return i


def _item_end(t: list, i: int) -> int:
    """Index past the item starting at `i`: its outer attributes, then up to a
    `;` or the close of its first `{…}` at depth 0."""
    i, depth = _past_attrs(t, i), 0
    while i < len(t):
        x = t[i]
        if x in "([{":
            depth += 1
        elif x in ")]}":
            depth -= 1
            if depth == 0 and x == "}":
                return i + 1
        elif x == ";" and depth == 0:
            return i + 1
        i += 1
    return i


def strip_tests(t: list) -> tuple:
    """The tokens without `#[cfg(test)]` items, and the `mod x;` names among them."""
    if t[:8] == ["#", "!", "[", "cfg", "(", "test", ")", "]"]:
        return [], []
    out, mods, i = [], [], 0
    while i < len(t):
        if t[i:i + 7] == TEST_ATTR:
            end = _item_end(t, i + 7)
            item = t[_past_attrs(t, i + 7):end]
            if item[:1] == ["pub"]:
                item = item[1:]
                if item[:1] == ["("]:
                    item = item[item.index(")") + 1:]
            if len(item) == 3 and item[0] == "mod" and item[2] == ";":
                mods.append(item[1])
            i = end
        else:
            out.append(t[i])
            i += 1
    return out, mods


def test_module_paths(rel: str, mods: list) -> list:
    """The files a file's test-only `mod x;` names, as path prefixes."""
    p = Path(rel)
    base = p.parent if p.name in ("mod.rs", "lib.rs", "main.rs") else p.parent / p.stem
    return [(base / f"{m}.rs").as_posix() for m in mods] + [(base / m).as_posix() + "/" for m in mods]


def digest_text(text: str) -> tuple:
    """(the token stream as text, the test-only module names it declares)."""
    tokens, mods = strip_tests(lex(text))
    return json.dumps(tokens, ensure_ascii=False), mods


def _self_test() -> int:
    bad = 0

    def check(name, ok):
        nonlocal bad
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}")
        bad += 0 if ok else 1

    same = lambda a, b: digest_text(a)[0] == digest_text(b)[0]
    base = 'fn f(a: u32) -> u32 {\n    a + 1 // one\n}\n'
    check("a comment moves nothing", same(base, 'fn f(a: u32) -> u32 { a + 1 }'))
    check("...nor a doc comment or a block comment",
          same("/// doc\n/* a /* nested */ b */\n" + base, base))
    check("...nor layout", same(base, "fn f( a:u32 )->u32{a+1}"))
    check("a changed number moves it", not same(base, base.replace("+ 1", "+ 2")))
    check("a changed operator moves it", not same(base, base.replace("+", "-")))
    check("a string keeps its comment-looking text",
          not same('const S: &str = "a // b";', 'const S: &str = "a ";'))
    check("...and its spaces", not same('const S: &str = "a b";', 'const S: &str = "a  b";'))
    check("a raw string is one token", lex('r#"x " // y"#') == ['r#"x " // y"#'])
    check("an escaped quote stays inside its string", lex('"a\\"b" c') == ['"a\\"b"', "c"])
    check("a char literal is not a lifetime", lex("'a' 'b") == ["'a'", "'b"])
    check("...nor an escaped one", lex("'\\'' x") == ["'\\''", "x"])
    check("a byte string is one token", lex('b"x" b\'y\'') == ['b"x"', "b'y'"])
    tests = base + "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { assert!(true); }\n}\n"
    check("a test module moves nothing", same(base, tests))
    check("...nor a test-only fn", same(base, base + "#[cfg(test)]\n#[allow(dead_code)]\npub fn helper() -> u8 { 0 }\n"))
    check("cfg(not(test)) is code", not same(base, base + "#[cfg(not(test))]\nfn g() {}\n"))
    check("a test-only `mod x;` is named", digest_text(base + "#[cfg(test)]\npub(crate) mod wide_beam;\n")[1] == ["wide_beam"])
    check("...and resolved beside its parent", test_module_paths("engine/src/fight/mod.rs", ["wide_beam"])[0] == "engine/src/fight/wide_beam.rs"
          and test_module_paths("engine/src/fight/run.rs", ["t"])[0] == "engine/src/fight/run/t.rs")
    check("an inner cfg(test) drops the file", digest_text("#![cfg(test)]\nfn f() {}")[0] == "[]")
    print(f"\n{bad} failed" if bad else "\nrust source is read as its tokens")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(_self_test() if "--self-test" in sys.argv else 2)
