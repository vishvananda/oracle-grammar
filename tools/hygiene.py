#!/usr/bin/env python3
"""Check owned source size, fixture closure and portable checked-in paths."""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
IGNORED = {'.git', '.cache', 'target', '__pycache__'}


def check(root=ROOT):
    errors = []
    files = [p for p in root.rglob('*')
             if p.is_file() and not any(part in IGNORED for part in p.relative_to(root).parts)]
    for path in files:
        if path.suffix not in ('.rs', '.py', '.md', '.toml', '.json', '.ebnf', '.yml', '.txt'):
            continue
        text = path.read_text()
        label = path.relative_to(root)
        if re.search(r'/(?:home|mnt)/[\w-]+/', text):
            errors.append(f'{label}: machine-specific path')
        if path.suffix == '.rs':
            if len(text.splitlines()) > 1500:
                errors.append(f'{label}: exceeds 1,500 lines')
            for name in re.findall(r'include_(?:str|bytes)!\(\s*"([^"]+)"', text):
                asset = (path.parent/name).resolve()
                if not asset.is_relative_to(root.resolve()) or not asset.is_file():
                    errors.append(f'{label}: missing or external embedded asset {name}')
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        raise ValueError(f'{len(errors)} hygiene checks failed')
    print(f'hygiene passed ({len(files)} source files and assets)')


if __name__ == '__main__':
    try:
        check()
    except (ValueError, OSError) as error:
        sys.exit(str(error))
