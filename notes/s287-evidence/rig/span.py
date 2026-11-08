"""s287: byte spans in a corpus file. usage: span.py FILE LINE_SUBSTRING NEEDLE [NTH]
Prints lo..hi of the NTH (1-based) occurrence of NEEDLE on the first line
containing LINE_SUBSTRING (skipping the //! header)."""
import sys
path, line_sub, needle = sys.argv[1:4]
nth = int(sys.argv[4]) if len(sys.argv) > 4 else 1
data = open(path, 'rb').read()
off = 0
for raw in data.split(b'\n'):
    line = raw.decode()
    if not line.startswith('//!') and line_sub in line:
        i = -1
        for _ in range(nth):
            i = line.index(needle, i + 1)
        print(f"({off + i}, {off + i + len(needle)})")
        sys.exit(0)
    off += len(raw) + 1
sys.exit("not found")
