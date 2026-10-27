#!/usr/bin/env python3
"""yw08 experiment: write fl_vm_exec's per-instruction helpers inline (C's
READ_BYTE/PUSH/POP macro shape). Reads src/fl/vm.lu, writes the result to argv[2]."""
import sys
src = open(sys.argv[1]).read()
start = src.index('fn fl_vm_exec(mut vm: FlVm, base: int) -> FlValue {')
end = src.index('\npub fn fl_vm_run(mut vm: FlVm, entry: FlValue) -> FlValue {')
head, body, tail = src[:start], src[start:end], src[end:]
n_then = body.count(' then fl_pop(mut vm) else fl_nil()')
body = body.replace(' then fl_pop(mut vm) else fl_nil()', ' { fl_pop(mut vm) } else { fl_nil() }')
R8 = '{ let rp = vm.frames[fi].ip; vm.frames[fi].ip = rp + 1; vm.gc.slots[fnidx].code[rp] as int }'
R16 = ('{ let rp = vm.frames[fi].ip; vm.frames[fi].ip = rp + 2; '
       'vm.gc.slots[fnidx].code[rp] as int | vm.gc.slots[fnidx].code[rp + 1] as int << 8 }')
POP = '{ if vm.sp <= 0 { fl_current_bug(mut vm, "fl vm: stack underflow") }; vm.sp -= 1; copy vm.stack[vm.sp] }'
counts = dict(read8=body.count('fl_read8(mut vm, fi, fnidx)'), read16=body.count('fl_read16(mut vm, fi, fnidx)'),
              pop=body.count('fl_pop(mut vm)'))
body = body.replace('fl_read8(mut vm, fi, fnidx)', R8).replace('fl_read16(mut vm, fi, fnidx)', R16)
body = body.replace('fl_pop(mut vm)', POP)
out, i, npush, key = [], 0, 0, 'fl_push(mut vm, '
while True:
    j = body.find(key, i)
    if j < 0:
        out.append(body[i:]); break
    out.append(body[i:j]); k = j + len(key); depth = 1; m = k
    while depth:
        c = body[m]
        if c == '(':
            depth += 1
        elif c == ')':
            depth -= 1
        elif c == '"':
            m += 1
            while body[m] != '"':
                m += 2 if body[m] == '\\' else 1
        m += 1
    out.append('{ let pv = %s; if vm.sp >= 16384 { fl_push_overflow(mut vm) } else { vm.stack[vm.sp] = pv; vm.sp += 1 } }'
               % body[k:m - 1])
    npush += 1; i = m
open(sys.argv[2], 'w').write(head + ''.join(out) + tail)
print('then-forms', n_then, counts, 'push', npush)
