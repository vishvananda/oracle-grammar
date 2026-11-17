"""Conservative linear-EBNF literal cohorts for mechanical removal checks."""
import re

import mined
import retire as r

MAX_BODY_BYTES = 64 * 1024
MAX_FILE_BYTES = 32 * 1024 * 1024
MAX_GRAMMAR_BYTES = 256 * 1024 * 1024
MAX_FILES = 4096
NAME = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')


def linear_words(body):
    """Read the complete supported sequence, or refuse an uncertain shape.

    This recognizes only quoted terminals and bare nonterminal names. W3C EBNF
    literals have no escapes: a backslash never hides their closing delimiter.
    Comments and all other grammar constructs are deliberately unsupported.
    """
    if not isinstance(body, str) or len(body.encode()) > MAX_BODY_BYTES:
        return None
    literals = []
    at = 0
    while at < len(body):
        if body[at].isspace():
            at += 1
            continue
        if body[at] in ('"', "'"):
            quote = body[at]
            end = body.find(quote, at + 1)
            if end < 0 or end == at + 1:
                return None
            literal = body[at + 1:end]
            if any(ord(char) < 32 for char in literal):
                return None
            literals.append(literal)
            at = end + 1
        else:
            name = NAME.match(body, at)
            if name is None:
                return None
            at = name.end()
    return re.findall(r'\w+', ' '.join(literals).lower())


def cohort(run, rule, index):
    """Select every matching face name; bounds and complete closure follow later."""
    paths = mined.grammar_files(run/'frozen')
    r.require(len(paths) <= MAX_FILES, 'mechanical grammar exceeds file bound')
    entries, total = [], 0
    for path in paths:
        r.require(not path.is_symlink() and path.is_file() and path.stat().st_size <= MAX_FILE_BYTES,
                  'mechanical grammar file is unavailable or too large')
        data = path.read_bytes()
        total += len(data)
        r.require(len(data) <= MAX_FILE_BYTES and total <= MAX_GRAMMAR_BYTES,
                  'mechanical grammar exceeds byte bound')
        lines = data.decode('utf-8').splitlines()
        found = mined.definitions(lines)
        if rule in found:
            declaration = found[rule]
            r.require(sum(mined.DEFINITION.match(line) is not None for _, line in declaration) == 1,
                      'candidate must have exactly one declaration')
            first, last = declaration[0][0], declaration[-1][0]
            # Preserve intervening comments so unsupported syntax cannot disappear.
            entries.append('\n'.join(lines[first:last + 1]).split('::=', 1)[1])
    r.require(len(entries) == 1, 'candidate must have exactly one declaration')
    words = linear_words(entries[0])
    if not words:
        return []
    def matches(text):
        tokens = iter(re.findall(r'\w+', text.lower()))
        return all(any(token == word for token in tokens) for word in words)
    return sorted({entry['name'] for entry in index.values() if matches(entry['text'])})
