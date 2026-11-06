"""Byte spans of the expected binder names: `spans.py FILE NEEDLE...`, where a
NEEDLE is `context|name`: the first occurrence of `context` (searched after the
previous needle's match) locates the binder, and `name` inside it is the span."""
import sys
path = sys.argv[1]
data = open(path, 'rb').read()
at = 0
for needle in sys.argv[2:]:
    context, name = needle.rsplit('|', 1)
    i = data.find(context.encode(), at)
    if i < 0:
        sys.exit(f"{path}: {context!r} not found after {at}")
    j = data.find(name.encode(), i)
    print(f"{name} {j} {j + len(name)}")
    at = i + len(context)
