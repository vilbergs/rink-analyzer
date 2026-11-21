#!/usr/bin/env bash
# Update the worker code and Rust binaries. The GPU environment, model, and secret must be present.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${RINK_WORKER_HOST:-furulid}"
ssh_opts=(-o BatchMode=yes -o ConnectTimeout=15)

ssh "${ssh_opts[@]}" "$host" 'mkdir -p "$HOME/.cache/rink-footage-build" "$HOME/.config/systemd/user"'
rsync -azR -e "ssh ${ssh_opts[*]}" Cargo.toml Cargo.lock crates/ scripts/detect-yolox.py scripts/footage-worker.py   deploy/furulid/rink-footage-worker.service "$host:.cache/rink-footage-build/"
ssh "${ssh_opts[@]}" "$host" 'bash -se' <<'REMOTE'
cd "$HOME/.cache/rink-footage-build"
worker="$HOME/.local/share/rink-detector"
test -r "$worker/yolox_s.onnx" && test -x "$worker/.venv/bin/python"
"$HOME/.cargo/bin/cargo" build --release --locked -p rink-footage --features tracking-preview   --bin rink-footage --bin rink-track-preview
for file in rink-footage rink-track-preview; do
  install -m 755 "target/release/$file" "$worker/$file.new"
  mv "$worker/$file.new" "$worker/$file"
done
install -m 644 scripts/detect-yolox.py scripts/footage-worker.py "$worker/"
install -m 644 deploy/furulid/rink-footage-worker.service "$HOME/.config/systemd/user/"
systemctl --user daemon-reload
if test -r "$HOME/.config/rink-footage-worker.env"; then
  systemctl --user enable rink-footage-worker.service
  systemctl --user restart rink-footage-worker.service
fi
REMOTE
