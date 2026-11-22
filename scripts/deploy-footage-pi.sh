#!/usr/bin/env bash
# Build the Rust footage service on the Pi and add it to the existing tailnet site.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${RINK_DEPLOY_HOST:-pi}"
jump="${RINK_DEPLOY_JUMP:-furulid}"
ssh_opts=(-o BatchMode=yes -o ConnectTimeout=15 -J "$jump")

ssh "${ssh_opts[@]}" "$host" 'mkdir -p "$HOME/.cache/rink-footage-build"'
rsync -azR -e "ssh ${ssh_opts[*]}" Cargo.toml Cargo.lock crates/ deploy/pi/rink-footage.service deploy/pi/rink.caddy   "$host:.cache/rink-footage-build/"

ssh "${ssh_opts[@]}" "$host" 'bash -se' <<'REMOTE'
cd "$HOME/.cache/rink-footage-build"
"$HOME/.cargo/bin/cargo" build --locked --release -p rink-footage --no-default-features --features web-import --bin rink-footage-web
sudo -n install -m 755 target/release/rink-footage-web /usr/local/bin/rink-footage-web.new
sudo -n mv /usr/local/bin/rink-footage-web.new /usr/local/bin/rink-footage-web
sudo -n install -m 644 deploy/pi/rink-footage.service /etc/systemd/system/rink-footage.service
sudo -n systemctl daemon-reload
sudo -n systemctl enable rink-footage.service
sudo -n systemctl restart rink-footage.service
curl --fail --silent --show-error --retry 6 --retry-connrefused --max-time 5 http://127.0.0.1:8794/api/footage/health
printf '\n'

backup=$(mktemp)
sudo -n cp /etc/caddy/rink.caddy "$backup"
sudo -n install -m 644 deploy/pi/rink.caddy /etc/caddy/rink.caddy
if ! sudo -n sh -c 'set -a; . /etc/caddy/cloudflare.env; caddy validate --config /etc/caddy/Caddyfile' ||
   ! sudo -n systemctl reload caddy; then
  sudo -n install -m 644 "$backup" /etc/caddy/rink.caddy
  exit 1
fi
rm "$backup"
REMOTE
printf 'https://rink.vilberg.dev/footage\n'
