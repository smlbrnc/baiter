#!/usr/bin/env bash
# btc-updown-5m-... için market_ticks → CSV indirimi (sunucuda DB üzerinden).
#
# Örnek (repo kökünde, sunucuda):
#   export DB_PATH="${DB_PATH:-./data/baiter.db}"
#   ./scripts/export_session_ticks.sh btc-updown-5m-1777467000
#
# İsteğe bağlı ikinci arg: bot_id (aynı slug birden fazla botta ise).
set -euo pipefail
SLUG="${1:?slug gerekli, örn: btc-updown-5m-1777467000}"
BOT_ID="${2:-}"
DB_PATH="${DB_PATH:-./data/baiter.db}"
OUT="${OUT:-./${SLUG}_ticks.csv}"

if [[ ! -f "$DB_PATH" ]]; then
  echo >&2 "DB bulunamadı: $DB_PATH  (veya DB_PATH=/path/to/baiter.db)"
  exit 1
fi

JOIN="JOIN market_sessions ms ON ms.id = mt.market_session_id"
WHERE="ms.slug = '$SLUG'"
if [[ -n "$BOT_ID" ]]; then
  WHERE+=" AND ms.bot_id = ${BOT_ID}"
fi

sqlite3 "$DB_PATH" <<SQL
.headers on
.mode csv
.output ${OUT}
SELECT
  mt.id,
  mt.bot_id,
  mt.market_session_id,
  mt.up_best_bid,
  mt.up_best_ask,
  mt.down_best_bid,
  mt.down_best_ask,
  mt.signal_score,
  mt.bsi,
  mt.ofi,
  mt.cvd,
  mt.ts_ms
FROM market_ticks mt
${JOIN}
WHERE ${WHERE}
ORDER BY mt.ts_ms ASC;
.output stdout
SQL

LINES="$(wc -l < "${OUT}" | tr -d ' ')"
ROWS=$((LINES > 1 ? LINES - 1 : 0))
echo "Yazıldı: ${OUT}  (${ROWS} tick)"
