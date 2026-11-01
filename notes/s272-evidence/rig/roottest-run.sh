#!/usr/bin/env bash
# s272 roottest, on the host (block A held by the caller): an image FROM
# ubuntu:24.04 with the programs COPIED in (no volume, no bind mount), one
# throwaway --network none container per case, each case as a user (4242)
# and as root inside the container; the nice cases also as root with
# --cap-add SYS_NICE (rootless: the capability is the container's own
# namespace's, not the host's). Pinned to block A inside the container.
set -u
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
R=/home/mfwolffe/lanes/s272/records/roottest
cd "$R" || exit 1
cat > Containerfile <<'EOC'
FROM docker.io/library/ubuntu:24.04
COPY t /t
RUN chmod -R a+rX /t && chmod a+x /t/wolf /t/bin/*/* \
 && useradd -m -u 4242 bore \
 && mkdir -p /w/jail && printf 'marker\n' > /w/jail/s272-marker \
 && chmod -R a+rX /w
WORKDIR /w
EOC
podman build -q -t localhost/s272-roottest:1 -f Containerfile . || exit 1
img=localhost/s272-roottest:1
podman image inspect $img --format '{{.Id}}'
n=0
run() { # who uid caps machine-cmd
  local who=$1 uid=$2 caps=$3; shift 3
  n=$((n+1))
  local capflag=""; [ "$caps" = none ] || capflag="--cap-add $caps"
  echo "=== case $n as $who caps=$caps: $*"
  podman run --rm --name "s272-rt-$$-$n" --network none --memory 12g --user "$uid" $capflag -e HOME=/tmp "$img" \
    taskset -c "$S272_CPUS" "$@"
  echo "@status $?"
}
for who in user:4242 root:0; do
  w=${who%%:*}; u=${who#*:}
  run $w $u none /t/wolf conform-run /t/fx/chroot_case.lu --checked --json
  run $w $u none /t/bin/native/chroot_case
  run $w $u none /t/bin/release/chroot_case
  for caps in none SYS_NICE; do
    run $w $u $caps /t/wolf conform-run /t/fx/priority_case.lu --checked --json
    run $w $u $caps /t/bin/native/priority_case
    run $w $u $caps /t/bin/release/priority_case
    run $w $u $caps sh -c 'nice -n -1 true; echo "gnu-nice status=$?"'
  done
  run $w $u none sh -c 'chroot /w/jail /bin/true; echo "gnu-chroot status=$?"'
done
podman rmi -f $img > /dev/null
echo "== done $(date -u +%FT%TZ), $n cases"
