"""Make sure <name> has an A record pointing at <ip> in Cloudflare.

Runs on the Pi as root, with the same token Caddy uses for its DNS challenge:

    sudo python3 -I ensure-dns.py rink.vilberg.dev 100.72.119.74
"""
import json
import sys
import urllib.request

name, ip = sys.argv[1], sys.argv[2]
zone_name = ".".join(name.split(".")[-2:])
env = {}
for line in open("/etc/caddy/cloudflare.env"):
    if "=" in line and not line.startswith("#"):
        k, v = line.strip().split("=", 1)
        env[k] = v.strip().strip('"')
token = env["CLOUDFLARE_API_TOKEN"]


def api(path, method="GET", body=None):
    req = urllib.request.Request(
        "https://api.cloudflare.com/client/v4" + path,
        data=json.dumps(body).encode() if body else None,
        method=method,
        headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=20) as r:
        out = json.load(r)
    if not out.get("success"):
        raise SystemExit(f"cloudflare {method} {path}: {out.get('errors')}")
    return out["result"]


zone = api(f"/zones?name={zone_name}")[0]["id"]
records = api(f"/zones/{zone}/dns_records?type=A&name={name}")
if records and records[0]["content"] == ip:
    print(f"dns ok: {name} -> {ip}")
elif records:
    api(f"/zones/{zone}/dns_records/{records[0]['id']}", "PATCH", {"content": ip})
    print(f"dns updated: {name} -> {ip} (was {records[0]['content']})")
else:
    api(f"/zones/{zone}/dns_records", "POST", {"type": "A", "name": name, "content": ip, "ttl": 300, "proxied": False})
    print(f"dns created: {name} -> {ip}")
