#!/usr/bin/env bash
# Публикация клиента и подписанного манифеста в Cloudflare R2.
#
# Требуется rclone с настроенным remote для R2. Один раз настрой:
#   rclone config create r2 s3 provider=Cloudflare \
#     access_key_id=<R2_ACCESS_KEY> secret_access_key=<R2_SECRET> \
#     endpoint=https://<ACCOUNT_ID>.r2.cloudflarestorage.com
#
# Переменные окружения:
#   CLIENT_DIR   путь к эталонной копии клиента (по умолчанию ./master-client)
#   R2_BUCKET    имя бакета R2 (например l2-client)
#   BASE_URL     публичный URL раздачи (например https://l2cdn.balabanets.uk/client/)
#   KEY_FILE     приватный ключ подписи (по умолчанию ~/.config/l2-launcher/keys/manifest.key)
#
# Пример:
#   R2_BUCKET=l2-client BASE_URL=https://l2cdn.balabanets.uk/client/ ./tools/publish.sh
set -euo pipefail

CLIENT_DIR="${CLIENT_DIR:-./master-client}"
KEY_FILE="${KEY_FILE:-$HOME/.config/l2-launcher/keys/manifest.key}"
R2_BUCKET="${R2_BUCKET:?задай R2_BUCKET}"
BASE_URL="${BASE_URL:?задай BASE_URL (например https://l2cdn.balabanets.uk/client/)}"
VERSION="${VERSION:-$(date +%Y.%m.%d)}"
OUT="./dist-manifest"

cd "$(dirname "$0")/.."

echo "==> 1/3 Генерация подписанного манифеста ($VERSION)"
cargo run --release -p manifest-gen --bin manifest-gen -- \
  --client "$CLIENT_DIR" --out "$OUT" \
  --base-url "$BASE_URL" --version "$VERSION" --key "$KEY_FILE"

echo "==> 2/3 Заливка файлов клиента в r2:$R2_BUCKET/client/"
rclone sync "$CLIENT_DIR" "r2:$R2_BUCKET/client/" \
  --transfers 8 --checkers 16 --fast-list --progress

echo "==> 3/3 Заливка манифеста и подписи"
rclone copy "$OUT/manifest.json"     "r2:$R2_BUCKET/client/" --progress
rclone copy "$OUT/manifest.json.sig" "r2:$R2_BUCKET/client/" --progress

echo "Готово. Манифест: ${BASE_URL}manifest.json"
