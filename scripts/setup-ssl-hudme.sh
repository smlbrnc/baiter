#!/usr/bin/env bash
# Let's Encrypt ile hudme.com + www için HTTPS (VPS'te bir kez çalıştır).
# Önkoşul: DNS A kaydı hudme.com ve www → bu sunucunun genel IP’si.
#
#   sudo apt-get install -y certbot python3-certbot-nginx
#   export LETSENCRYPT_EMAIL=senin@email.com
#   ./scripts/setup-ssl-hudme.sh
#
set -euo pipefail

: "${LETSENCRYPT_EMAIL:?LETSENCRYPT_EMAIL ayarla (Let's Encrypt uyarıları için)}"

sudo certbot --nginx \
  -d hudme.com -d www.hudme.com \
  --non-interactive --agree-tos -m "$LETSENCRYPT_EMAIL" \
  --redirect

echo "HTTPS hazır. Sonra: cd /opt/baiter-pro && ./scripts/deploy-vps.sh"
