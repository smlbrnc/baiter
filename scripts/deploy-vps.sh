#!/usr/bin/env bash
# Deploy baiter-pro on the VPS from GitHub (run as `ubuntu` over SSH).
#
# Dışarıdan http://SUNUCU_IP açılmıyorsa: AWS EC2 → Security group → Inbound rules
# ekle: Type "HTTP" TCP 80, Source 0.0.0.0/0 (ve TLS için HTTPS 443). UFW sunucuda
# zaten açık; trafik çoğunlukla SG'de takılır.
#
# Kullanım (VPS'te):
#   chmod +x scripts/deploy-vps.sh
#   ./scripts/deploy-vps.sh
#
# Örnek (domain ile SSE URL):
#   BAITER_PUBLIC_URL=https://panel.example.com ./scripts/deploy-vps.sh
#
set -euo pipefail

APP_DIR="${APP_DIR:-/opt/baiter-pro}"
GIT_REMOTE="${GIT_REMOTE:-origin}"
GIT_BRANCH="${GIT_BRANCH:-main}"
BAITER_PUBLIC_URL="${BAITER_PUBLIC_URL:-http://52.18.245.113}"

cd "$APP_DIR"

git fetch "$GIT_REMOTE"
git checkout "$GIT_BRANCH"
git reset --hard "${GIT_REMOTE}/${GIT_BRANCH}"

export PATH="$HOME/.cargo/bin:$PATH"
. "$HOME/.cargo/env" 2>/dev/null || true

cargo build --release

cd frontend
npm install
export BAITER_BACKEND_URL=http://127.0.0.1:3000
export NEXT_PUBLIC_BAITER_SSE_URL="${BAITER_PUBLIC_URL}/api/events"
npm run build

sudo systemctl restart baiter-supervisor baiter-frontend
sudo nginx -t
sudo systemctl reload nginx

echo "--- local health ---"
curl -sS -m 5 "http://127.0.0.1/api/health" || true
echo
echo "Deploy finished. If the site is still unreachable from the internet, open TCP 80 (and 443) in the EC2 security group."
