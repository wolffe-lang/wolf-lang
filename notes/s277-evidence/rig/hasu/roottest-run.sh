#!/usr/bin/env bash
# s277 roottest, on the host (the block held by the caller; adapted by copy
# from s272's roottest-run.sh): an image FROM ubuntu:24.04 with the
# programs COPIED in (no volume, no bind mount) and the privilege layout
# made as the image's root; one throwaway --network none container per
# case, each case as a user (4242) and as root inside the container, each
# machine (checked, native, release) beside GNU's ln, chmod, mkdir and rm.
set -u
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
R=/home/mfwolffe/lanes/s277/records/roottest
cd "$R" || exit 1
cat > Containerfile <<'EOC'
FROM docker.io/library/ubuntu:24.04
COPY t /t
RUN chmod -R a+rX /t && chmod a+x /t/wolf /t/bin/* \
 && useradd -m -u 4242 bore \
 && P=/tmp/s277-priv && mkdir -p $P/rootdir $P/sticky $P/userdir \
 && echo root > $P/rootdir/rootfile && echo root2 > $P/rootdir/rootfile2 \
 && chmod 644 $P/rootdir/rootfile $P/rootdir/rootfile2 && chmod 755 $P $P/rootdir \
 && chmod 1777 $P/sticky && echo root > $P/sticky/rootfile && chmod 644 $P/sticky/rootfile \
 && echo user > $P/userdir/userfile && chown -R 4242:4242 $P/userdir && chmod 755 $P/userdir
WORKDIR /t/fx
EOC
podman build -q -t localhost/s277-roottest:1 -f Containerfile . || exit 1
img=localhost/s277-roottest:1
podman image inspect $img --format '{{.Id}}'
echo "host protected_hardlinks=$(cat /proc/sys/fs/protected_hardlinks)"
n=0
run() { # who uid cmd...
  local who=$1 uid=$2; shift 2
  n=$((n+1))
  echo "=== case $n as $who: $*"
  podman run --rm --name "s277-rt-$$-$n" --network none --memory 4g --user "$uid" -e HOME=/tmp "$img" \
    taskset -c "$S277_CPUS" "$@"
  echo "@status $?"
}
for who in user:4242 root:0; do
  w=${who%%:*}; u=${who#*:}
  run $w $u /t/wolf conform-run /t/fx/privilege.lu --checked --json
  run $w $u /t/bin/privilege-native
  run $w $u /t/bin/privilege-release
  run $w $u sh -c 'P=/tmp/s277-priv; ln $P/rootdir/rootfile $P/userdir/gnu-mine; echo "gnu-ln-across status=$?"; ln $P/userdir/userfile $P/rootdir/gnu-theirs; echo "gnu-ln-into status=$?"; chmod 600 $P/rootdir/rootfile; echo "gnu-chmod status=$?"; mkdir $P/rootdir/gnu-x; echo "gnu-mkdir status=$?"; rm -f $P/sticky/rootfile; echo "gnu-rm-sticky status=$?"'
done
podman rmi -f $img > /dev/null
echo "== done $(date -u +%FT%TZ), $n cases"
