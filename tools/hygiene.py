#!/usr/bin/env python3
"""Check owned source size, fixture closure and portable checked-in paths."""
from pathlib import Path
import os
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
IGNORED = {'.git', '.cache', 'target', '__pycache__'}


def check(root=ROOT):
    errors = []
    files = []
    for directory, subdirs, names in os.walk(root):
        subdirs[:] = sorted(name for name in subdirs if name not in IGNORED
                            and not (Path(directory)/name).is_symlink())
        files.extend(Path(directory)/name for name in sorted(names))
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
