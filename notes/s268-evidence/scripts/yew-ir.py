import collections
import json
from pathlib import Path
import re
base=Path('/home/mfwolffe/lanes/s268/evidence/yew')
for variant in ['control','inline']:
    text=(base/f'{variant}.ll').read_text()
    body=re.search(r'^define[^\n]*_Wfl\.fl_vm_exec\$[^\n]*\n.*?^}',text,re.M|re.S)
    if body is None: raise RuntimeError('missing VM IR')
    opcodes=collections.Counter()
    calls=collections.Counter()
    for line in body.group().splitlines()[1:-1]:
        line=line.strip()
        line=re.sub(r'^%[^ ]+ = ', '', line)
        if not line or line.startswith(';') or line.endswith(':'): continue
        opcodes[line.split()[0]]+=1
        if ' call ' in ' '+line:
            target=re.search(r'@"?([^" (]+)',line)
            if target: calls[target.group(1)]+=1
    print(json.dumps(dict(variant=variant,header=body.group().splitlines()[0],opcodes=dict(opcodes),calls=dict(calls)),indent=2))
