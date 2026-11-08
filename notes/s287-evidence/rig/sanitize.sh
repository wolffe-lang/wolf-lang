#!/bin/bash
# s287: copy a lane log into the tree's evidence (adapted by copy from s285's
# sanitize.sh). The lane's paths become $LANE, the org checkouts $ORG, another
# lane's lock line loses its lane id, the Linux host's name (LINUX_HOST) is
# written H1, the two macOS machines M1 and M2, and any line naming a private repository (PRIVATE_RE, an
# extended regex) is dropped; its evidence stays in the lane.
# usage: LINUX_HOST=... PRIVATE_RE=... sanitize.sh IN OUT
: "${LINUX_HOST:?set LINUX_HOST}" "${PRIVATE_RE:?set PRIVATE_RE}"
sed -E -e 's#/Users/[a-z]*/lanes/s287/org#$ORG#g' -e 's#/Users/[a-z]*/lanes/s287#$LANE#g' \
    -e 's#/home/[a-z]*/lanes/s287#$LANE#g' -e 's#/home/[a-z]*/lanes/#$LANES/#g' \
    -e 's#mac build lock held by [A-Za-z0-9_-]* pid [0-9]*#mac build lock held by another lane#' \
    -e "s#${LINUX_HOST}#H1#g" -e "s#/Users/[a-z]*/#\$HOME/#g" -e "s#/home/[a-z]*/#\$HOME/#g" \
    -e 's#M2(\.local)?#M2#g' -e 's#M1(\.local)?#M1#g' -e 's#user#user#g' \
    "$1" | grep -Ev "${PRIVATE_RE}" > "$2"
