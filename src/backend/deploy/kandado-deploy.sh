#!/usr/bin/env bash
# Installs one Kandado server release on the droplet and switches to it
# (decision 16). Run by the GitHub Actions deploy workflow over SSH, as the
# deploy user, which owns /opt/kandado and may run exactly
# `sudo systemctl restart kandado-server` (see the sudoers line in DEPLOY.md).
#
#   kandado-deploy.sh /path/to/kandado-server-<sha>.tar.gz <sha> < google.env
#
# Standard input may carry the Google sign-in client as two lines,
# GOOGLE_CLIENT_ID=… and GOOGLE_CLIENT_SECRET=… (from GitHub secrets). They are
# written to /opt/kandado/config/google.env, which the service reads and only
# the deploy user writes. Empty input leaves that file as it is. The values are
# never printed.
#
# The release is unpacked to /opt/kandado/releases/<sha>, `current` is pointed
# at it, and the service restarts. If /readyz does not answer within 60 s,
# `current` goes back to the previous release and the service restarts again.
# The last five releases are kept.
set -euo pipefail

archive=$1
sha=$2
# The three overrides are for testing the script away from the droplet.
root=${KANDADO_ROOT:-/opt/kandado}
releases=$root/releases
target=$releases/$sha
ready_url=${KANDADO_READY_URL:-http://127.0.0.1:8081/readyz}
restart=${KANDADO_RESTART:-sudo /usr/bin/systemctl restart kandado-server}

[[ $sha =~ ^[0-9a-f]{7,40}$ ]] || { echo "not a commit id: $sha" >&2; exit 2; }
[[ -f $archive ]] || { echo "no such archive: $archive" >&2; exit 2; }

# Google sign-in settings, if the pipeline sent them.
google=""
[[ -t 0 ]] || google=$(cat)
if [[ -n $google ]]; then
  # Exactly the two keys, each with a value and no spaces: nothing else gets in.
  if [[ $(printf '%s\n' "$google" | grep -cE '^GOOGLE_CLIENT_ID=[^[:space:]]+$') -ne 1 ]] \
    || [[ $(printf '%s\n' "$google" | grep -cE '^GOOGLE_CLIENT_SECRET=[^[:space:]]+$') -ne 1 ]] \
    || [[ $(printf '%s\n' "$google" | grep -c .) -ne 2 ]]; then
    echo "the Google settings are not two GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET lines" >&2
    exit 2
  fi
  mkdir -p "$root/config"
  (umask 027 && printf '%s\n' "$google" > "$root/config/google.env.tmp")
  mv -f "$root/config/google.env.tmp" "$root/config/google.env"
  echo "Google sign-in settings updated"
fi
unset google

mkdir -p "$releases"
rm -rf "$target.tmp"
mkdir -p "$target.tmp"
tar -xzf "$archive" -C "$target.tmp"
[[ -x $target.tmp/kandado-server ]] || { echo "the archive has no kandado-server" >&2; exit 2; }
# The service runs as `kandado`: it reads the release, never writes it.
chmod -R go-w "$target.tmp"
chmod -R a+rX "$target.tmp"
rm -rf "$target"
mv "$target.tmp" "$target"

previous=$(readlink -f "$root/current" 2>/dev/null || true)

switch_to() {
  ln -sfn "$1" "$root/current.next"
  mv -Tf "$root/current.next" "$root/current"
  $restart
}

wait_ready() {
  for _ in $(seq 1 "${KANDADO_READY_TRIES:-60}"); do
    if curl -fsS --max-time 2 "$ready_url" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  return 1
}

echo "switching to $sha"
switch_to "$target"
if wait_ready; then
  echo "kandado-server $sha is ready"
else
  echo "kandado-server $sha did not become ready" >&2
  if [[ -n $previous && -d $previous && $previous != "$target" ]]; then
    echo "rolling back to $(basename "$previous")" >&2
    switch_to "$previous"
    wait_ready && echo "rolled back" >&2 || echo "the previous release did not come back either" >&2
  fi
  exit 1
fi

# Keep the five newest releases, and never the one in use.
current=$(readlink -f "$root/current")
ls -1dt "$releases"/*/ 2>/dev/null | tail -n +6 | while read -r old; do
  old=${old%/}
  [[ $old == "$current" ]] || rm -rf "$old"
done
rm -f "$archive"
