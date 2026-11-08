#!/usr/bin/env bash
set -euo pipefail

HOST=${DEPLOY_HOST:-vps}
ROOT=/var/www/kurumi.world
URL=https://kurumi.world
TARGET=x86_64-unknown-linux-musl
BIN=guestbook/target/$TARGET/release/guestbook

cd "$(dirname "$(readlink -f "$0")")"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<EOF
usage: ./deploy.sh [command]

  all          push, update the page, rebuild + restart the guestbook if it changed (default)
  site         only push and update the page
  guestbook    only rebuild + upload the guestbook server
  status       is everything up?
  logs [-f]    last guestbook log lines

DEPLOY_HOST overrides the ssh host (default: vps).
EOF
}

push() {
  [[ $(git branch --show-current) == master ]] || die "not on master"
  [[ -z $(git status --porcelain) ]] || die "uncommitted changes, commit them first"
  say "pushing to github"
  git push -q origin master
}

site() {
  say "updating the page on $HOST"
  ssh "$HOST" "runuser -u www-data -- git -C $ROOT pull -q --ff-only &&
               runuser -u www-data -- git -C $ROOT log --oneline -1"
}

guestbook() {
  say "building guestbook"
  (cd guestbook && cargo build -q --release --target "$TARGET")
  local here there
  here=$(sha256sum "$BIN" | cut -d' ' -f1)
  there=$(ssh "$HOST" 'sha256sum /usr/local/bin/guestbook 2>/dev/null | cut -d" " -f1' || true)
  if [[ $here == "$there" ]]; then
    say "guestbook is already up to date"
    return
  fi
  say "uploading guestbook and restarting it"
  scp -q "$BIN" "$HOST":/tmp/guestbook.new
  ssh "$HOST" 'install -m 755 /tmp/guestbook.new /usr/local/bin/guestbook &&
               rm /tmp/guestbook.new &&
               systemctl restart guestbook && sleep 1 &&
               systemctl is-active --quiet guestbook' || die "guestbook did not start, see ./deploy.sh logs"
}

status() {
  say "checking $URL"
  local ok=1 code
  for path in / /keys /api/guestbook; do
    code=$(curl -s -o /dev/null -w '%{http_code}' "$URL$path")
    printf '  %-16s %s\n' "$path" "$code"
    [[ $code == 200 ]] || ok=0
  done
  printf '  %-16s %s\n' "guestbook" "$(ssh "$HOST" systemctl is-active guestbook || true)"
  ((ok)) || die "something is down"
}

case ${1:-all} in
  all)       push; site; guestbook; status ;;
  site)      push; site; status ;;
  guestbook) guestbook; status ;;
  status)    status ;;
  logs)      shift; ssh -t "$HOST" journalctl -u guestbook -n 50 --no-pager "$@" ;;
  *)         usage; exit 1 ;;
esac
