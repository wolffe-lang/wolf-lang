#!/bin/bash
# Copy a lane log into the tree's evidence: the lane's paths become $LANE,
# another lane's lock line loses its lane id, and any line naming the one
# private downstream consumer is dropped (its evidence stays in the lane).
# usage: sanitize.sh IN OUT
sed -e 's#$HOME/lanes/s282#$LANE#g' -e 's#/home/$USER/lanes/s282#$LANE#g' \
    -e 's#mac build lock held by [A-Za-z0-9_-]* pid [0-9]*#mac build lock held by another lane#' \
    -e "/${PRIVATE_NAME:?the private consumer's name}/d" "$1" > "$2"
