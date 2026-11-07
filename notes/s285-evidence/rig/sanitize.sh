#!/bin/bash
# s285: copy a lane log into the tree's evidence (adapted by copy from s282's
# sanitize.sh). The lane's paths become $LANE, the org checkouts $ORG, another
# lane's lock line loses its lane id, the Linux host's name (LINUX_HOST) is
# written H1, and any line naming the one private downstream consumer
# (PRIVATE_NAME) is dropped; its evidence stays in the lane.
# usage: LINUX_HOST=... PRIVATE_NAME=... sanitize.sh IN OUT
: "${LINUX_HOST:?set LINUX_HOST}" "${PRIVATE_NAME:?set PRIVATE_NAME}"
sed -e 's#/Users/[a-z]*/lanes/s285/org#$ORG#g' -e 's#/Users/[a-z]*/lanes/s285#$LANE#g' \
    -e 's#/home/[a-z]*/lanes/s285#$LANE#g' -e 's#/home/[a-z]*/lanes/#$LANES/#g' \
    -e 's#mac build lock held by [A-Za-z0-9_-]* pid [0-9]*#mac build lock held by another lane#' \
    -e "s#${LINUX_HOST}#H1#g" -e "s#/Users/[a-z]*/#\$HOME/#g" -e "s#/home/[a-z]*/#\$HOME/#g" \
    -e "/${PRIVATE_NAME}/d" "$1" > "$2"
