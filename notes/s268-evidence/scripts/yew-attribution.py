import collections
import json
from pathlib import Path
import re
base = Path('/home/mfwolffe/lanes/s268/evidence/yew')
for variant in ['control','inline']:
    text = (base/f'{variant}.asm').read_text()
    match = re.search(r'^([0-9a-f]+) <(_Wfl\.fl_vm_exec\$[^>]+)>:\n(.*?)(?=\n\n)', text, re.M|re.S)
    if not match: raise RuntimeError('missing VM assembly')
    body = match.group(0)
    (base/f'{variant}-vm.asm').write_text(body+'\n')
    instructions = []
    for line in match.group(3).splitlines():
        fields = line.split('\t')
        if len(fields) >= 3 and re.match(r'^\s*[0-9a-f]+:', fields[0]):
            instructions.append(fields[-1])
    calls = collections.Counter()
    for instruction in instructions:
        if instruction.startswith('call'):
            target = re.search(r'<([^>]+)>', instruction)
            calls[target.group(1) if target else instruction] += 1
    result = dict(variant=variant, instructions=len(instructions), stack_references=sum(bool(re.search(r'\(%[re]?(?:sp|bp)\)', i)) for i in instructions), stack_adjustments=[i for i in instructions if re.match(r'(sub|add)\s+\$[^,]+,%rsp',i)], calls=dict(calls), returns=sum(i.startswith('ret') for i in instructions), external_tail_jumps=[i for i in instructions if i.startswith('jmp') and '<' in i and match.group(2) not in i])
    print(json.dumps(result,indent=2))
    print('\n'.join(body.splitlines()[:38]))
