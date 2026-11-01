#!/usr/bin/env bash
# Put the built web app on the Pi behind Caddy at https://rink.vilberg.dev (tailnet only).
# Needs web/dist (the web-build workflow), SSH to the Pi through furulid, and sudo on the Pi.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${RINK_DEPLOY_HOST:-pi}"
jump="${RINK_DEPLOY_JUMP:-furulid}"
site="${RINK_SITE:-rink.vilberg.dev}"
root=/var/www/rink
ssh_opts=(-o BatchMode=yes -o ConnectTimeout=15 -o StrictHostKeyChecking=accept-new -J "$jump")

[ -f web/dist/index.html ] || { echo "web/dist is missing; run: swamp workflow run web-build" >&2; exit 1; }

ssh "${ssh_opts[@]}" "$host" "sudo -n install -d -o \$USER -g \$USER $root"
rsync -az --delete -e "ssh ${ssh_opts[*]}" web/dist/ "$host:$root/"
rsync -az -e "ssh ${ssh_opts[*]}" deploy/pi/rink.caddy deploy/pi/ensure-dns.py "$host:/tmp/"
ssh "${ssh_opts[@]}" "$host" "
  set -e
  ip=\$(tailscale ip -4)
  dns=\$(sudo -n python3 -I /tmp/ensure-dns.py $site \"\$ip\"); echo \"\$dns\"
  case \"\$dns\" in 'dns ok'*) ;; *) command -v pihole >/dev/null && sudo -n pihole reloaddns >/dev/null 2>&1 && echo 'pihole cache flushed' ;; esac
  sudo -n install -m 644 /tmp/rink.caddy /etc/caddy/rink.caddy
  grep -q 'import /etc/caddy/rink.caddy' /etc/caddy/Caddyfile || echo 'import /etc/caddy/rink.caddy' | sudo -n tee -a /etc/caddy/Caddyfile >/dev/null
  sudo -n sh -c 'set -a; . /etc/caddy/cloudflare.env; caddy validate --config /etc/caddy/Caddyfile' 2>&1 | grep -v '^{' || true
  sudo -n sh -c 'set -a; . /etc/caddy/cloudflare.env; caddy validate --config /etc/caddy/Caddyfile' >/dev/null 2>&1
  sudo -n systemctl reload caddy
  rm -f /tmp/rink.caddy /tmp/ensure-dns.py
  echo https://$site/
"
