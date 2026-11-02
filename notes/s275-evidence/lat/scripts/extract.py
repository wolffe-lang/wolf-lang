import json, shutil, sys
out = sys.argv[1]
exe = None
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "wolf_rt" and m.get("profile", {}).get("test"):
        exe = m["executable"]
if not exe:
    sys.exit("no wolf_rt lib test executable")
shutil.copy2(exe, out)
print("copied", exe, "->", out)
