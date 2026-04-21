#!/usr/bin/env bash
# Certbot ile hudme.com + www icin HTTPS (VPS uzerinde bir kez calistir).
# Mac teki .pem ile VPS e bir kez baglan; VPS icindeyken tekrar ssh ETME.
# Onkosul: DNS A kaydi hudme.com ve www -> bu sunucunun public IP si.
#
#   sudo apt-get install -y certbot python3-certbot-nginx
#   export LETSENCRYPT_EMAIL=senin@email.com
#   ./scripts/setup-ssl-hudme.sh
#
set -euo pipefail

: "${LETSENCRYPT_EMAIL:?LETSENCRYPT_EMAIL ortam degiskenini ayarlayin (Certbot eposta)}"

sudo certbot --nginx \
  -d hudme.com -d www.hudme.com \
  --non-interactive --agree-tos -m "$LETSENCRYPT_EMAIL" \
  --redirect

echo "HTTPS hazır. Sonra: cd /opt/baiter-pro && ./scripts/deploy-vps.sh"
