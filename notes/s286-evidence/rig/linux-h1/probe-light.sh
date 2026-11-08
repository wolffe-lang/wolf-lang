#!/usr/bin/env bash
# s286 on H1, a light job (run through H1-env/bin/light B): the arena
# probe in the toolchain image, plain and as a set-user-ID copy run by
# another user, with no setting, MALLOC_ARENA_MAX=2 and the tunable; and
# the plain probe under the light CPUs alone (2 of the host's 20).
#   probe-light.sh LOG
set -uo pipefail
base=$LANE
log=$1
export PATH=$LANES/H1-env/bin:$LANES/H1-env/profile/bin:$PATH
podman run --rm --name s286-probe --memory "${LIGHT_MEM:-6g}" --pids-limit 4096 --ulimit core=0 \
  --userns keep-id:uid=1000,gid=1000 --user 0 -v "$base:/lane:rw" -e CPUS="${LIGHT_CPUS:?}" \
  localhost/r41-toolchain:1 bash -c '
    set -u
    /usr/bin/taskset.real -c "$CPUS" cc -O2 -pthread /lane/rig/arena-probe.c -o /opt/arena-probe || exit $?
    cp /opt/arena-probe /opt/arena-probe-suid && chown 0:0 /opt/arena-probe-suid && chmod 4755 /opt/arena-probe-suid
    ldd --version | head -1; uname -r; nproc
    cat > /opt/legs.sh <<"LEGS"
for exe in /opt/arena-probe /opt/arena-probe-suid; do
  for env in none MALLOC_ARENA_MAX=2 GLIBC_TUNABLES=glibc.malloc.arena_max=2; do
    if [ "$env" = none ]; then out=$("$exe" 2>&1); else out=$(env "$env" "$exe" 2>&1); fi
    printf "probe %s %s: arenas=%s | %s\n" "${exe##*/}" "$env" "$(printf "%s\n" "$out" | grep -c "^Arena ")" "$(printf "%s\n" "$out" | grep "^PROBE")"
  done
done
LEGS
    /usr/bin/taskset.real -c "$CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --inh-caps=-all --ambient-caps=-all bash /opt/legs.sh
    exit $?' > "$log" 2>&1
echo "probe-light exit=$?" >> "$log"
