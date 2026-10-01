"""Writes gfm-0.29-extensions.json: the extension examples of the GFM spec
(tables, strikethrough, autolinks, task lists) in spec.json's shape.

    curl -fsSL -o spec.txt https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt
    python3 gfm_extensions.py spec.txt
"""
import json
import sys

FENCE = "`" * 32 + " example"
lines = open(sys.argv[1], encoding="utf-8").read().split("\n")
examples, section, number, i = [], "", 0, 0
while i < len(lines):
    line = lines[i]
    if line.startswith("#"):
        section = line.lstrip("#").strip().removesuffix(" (extension)")
    if line.startswith(FENCE):
        number += 1
        kind = line[len(FENCE):].strip()
        end = lines.index("`" * 32, i + 1)
        dot = lines.index(".", i + 1)
        md, html = lines[i + 1 : dot], lines[dot + 1 : end]
        tasks = kind == "disabled" and section == "Task list items"
        if kind in ("table", "strikethrough", "autolink") or tasks:
            examples.append({
                "markdown": "".join(l + "\n" for l in md).replace("→", "\t"),
                "html": "".join(l + "\n" for l in html).replace("→", "\t"),
                "example": number,
                "section": section,
            })
        i = end
    i += 1
json.dump(examples, open("gfm-0.29-extensions.json", "w"), indent=2, ensure_ascii=False)
print(len(examples), "examples")
