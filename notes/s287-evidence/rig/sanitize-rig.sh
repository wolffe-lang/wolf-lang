#!/bin/bash
# s287: a rig file for the tree's evidence. As sanitize.sh, and the second
# macOS machine's name and access path become M2, the Linux hosts H1 (the
# first, H1's role) and H2, and the private trees' names one placeholder.
# usage: PRIVATE_LIST="a b c" sanitize-rig.sh IN OUT
: "${PRIVATE_LIST:?set PRIVATE_LIST}"
s=$(sed -E -e 's#/Users/[a-z]*/lanes/s287/org#$ORG#g' -e 's#/Users/[a-z]*/lanes/s287#$LANE#g' \
    -e 's#/home/[a-z]*/lanes/s287#$LANE#g' -e 's#$LANE#$LANE#g' \
    -e 's#$LANES/#$LANES/#g' -e 's#/home/[a-z]*/lanes/#$LANES/#g' -e 's#/Users/[a-z]*/#$HOME/#g' \
    -e 's#M2#M2#g' -e 's#M2(\.local)?#M2#g' -e 's#ssh-m2#ssh-m2#g' \
    -e 's#m2-env#m2-env#g' -e 's#known_hosts\.air#known_hosts.m2#g' -e 's#orch#orch#g' \
    -e 's#M2#M2#g' -e 's#M2#M2#g' -e 's#m2-locked#m2-locked#g' -e 's#m2-run#m2-run#g' \
    -e 's#M2-EXIT#M2-EXIT#g' -e 's#last-release-m2#last-release-m2#g' \
    -e 's#H2#H2#g' -e 's#H1#H1#g' -e 's#M1#M1#g' -e 's#user#user#g' "$1")
printf '%s\n' "$s" | python3 -I -c '
import re, sys
names = sys.argv[1].split()
text = sys.stdin.read()
for n in names:
    text = re.sub(r"(?<![A-Za-z0-9_-])" + re.escape(n) + r"(?![A-Za-z0-9_-])", "PRIVATE", text)
text = re.sub(r"PRIVATE(?:[ ,]+PRIVATE)+", "PRIVATE", text)
sys.stdout.write(text)
' "$PRIVATE_LIST" > "$2"
