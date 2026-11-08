#!/bin/bash
# s286: copy a lane log into the tree's evidence (adapted by copy from
# s285's sanitize.sh). The lane's paths on every host become $LANE, other
# lanes' paths $LANES, home directories $HOME; the 20-CPU Linux host is
# written H1 and the 16-thread one H2; the first Mac's and the second Mac's
# names are dropped; another lane's id in a lock line is dropped.
# usage: H1_NAME=... H2_NAME=... MAC2_USER=... MAC2_HOST=... sanitize.sh IN OUT
: "${H1_NAME:?}" "${H2_NAME:?}" "${MAC2_USER:?}" "${MAC2_HOST:?}"
sed -e 's#/Users/[a-z]*/lanes/s286#$LANE#g' -e 's#/home/[a-z]*/lanes/s286#$LANE#g' \
    -e 's#$LANE#$LANE#g' -e 's#$LANES/#$LANES/#g' -e 's#$LANES#$LANES#g' \
    -e 's#/home/[a-z]*/lanes/#$LANES/#g' -e 's#/Users/[a-z]*/lanes/#$LANES/#g' \
    -e 's#\(lock\) held by [A-Za-z0-9_-]* pid [0-9]*#\1 held by another lane#g' \
    -e 's#waiting on [A-Za-z0-9_-]* pid [0-9]*#waiting on another lane#g' \
    -e "s#${H1_NAME}#H1#g" -e "s#${H2_NAME}#H2#g" -e "s#${MAC2_HOST}#the-second-mac#g" -e "s#${MAC2_USER}#user#g" \
    -e 's#/Users/[a-z]*/#$HOME/#g' -e 's#/home/[a-z]*/#$HOME/#g' \
    -e 's#/Users/[a-z][a-z]*#$HOME#g' -e 's#/home/[a-z][a-z]*#$HOME#g' -e 's#the-first-mac#the-first-mac#g' "$1" > "$2"
