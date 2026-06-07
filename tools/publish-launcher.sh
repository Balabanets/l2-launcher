#!/usr/bin/env bash
# Публикация подписанных метаданных самообновления лаунчера.
#
# После того как CI собрал релиз vX.Y.Z (и залил L2-Interlude-portable.exe),
# этот скрипт:
#   1. скачивает портативный exe из релиза,
#   2. считает SHA-256 и размер,
#   3. формирует launcher.json {version, exe_url, sha256, size},
#   4. подписывает его Ed25519 нашим ключом (тем же, что манифест),
#   5. заливает launcher.json + launcher.json.sig в тот же релиз.
#
# Лаунчер читает их по алиасу releases/latest/download и проверяет подпись
# ВШИТЫМ публичным ключом — подменить обновление на сервере нельзя.
#
# Использование: tools/publish-launcher.sh v0.4.0
set -euo pipefail

TAG="${1:?Укажите тег релиза, напр.: tools/publish-launcher.sh v0.4.0}"
REPO="Balabanets/l2-launcher"
KEY="${MANIFEST_KEY:-$HOME/.config/l2-launcher/keys/manifest.key}"
EXE_ASSET="L2-Interlude-portable.exe"
VERSION="${TAG#v}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

EXE_URL="https://github.com/${REPO}/releases/download/${TAG}/${EXE_ASSET}"

echo "→ Скачиваю $EXE_ASSET из релиза $TAG…"
gh release download "$TAG" --repo "$REPO" --pattern "$EXE_ASSET" --dir "$WORK" --clobber

SHA="$(sha256sum "$WORK/$EXE_ASSET" | cut -d' ' -f1)"
SIZE="$(stat -c%s "$WORK/$EXE_ASSET")"
echo "  sha256=$SHA size=$SIZE"

cat > "$WORK/launcher.json" <<JSON
{
  "version": "${VERSION}",
  "exe_url": "${EXE_URL}",
  "sha256": "${SHA}",
  "size": ${SIZE}
}
JSON

echo "→ Подписываю launcher.json (Ed25519)…"
KEY_PATH="$KEY" JSON_PATH="$WORK/launcher.json" SIG_PATH="$WORK/launcher.json.sig" node - <<'NODE'
const crypto = require("crypto");
const fs = require("fs");
const keyRaw = fs.readFileSync(process.env.KEY_PATH, "utf8").trim();
// Ключ: 32 байта seed в hex или base64.
let seed;
if (/^[0-9a-fA-F]{64}$/.test(keyRaw)) seed = Buffer.from(keyRaw, "hex");
else seed = Buffer.from(keyRaw, "base64");
if (seed.length !== 32) throw new Error("ключ должен быть 32 байта, получено " + seed.length);
// PKCS8-обёртка для Ed25519 seed.
const der = Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]);
const key = crypto.createPrivateKey({ key: der, format: "der", type: "pkcs8" });
const data = fs.readFileSync(process.env.JSON_PATH);
const sig = crypto.sign(null, data, key); // 64 байта
fs.writeFileSync(process.env.SIG_PATH, sig.toString("base64"));
console.log("  подпись: " + sig.length + " байт → base64");
NODE

echo "→ Заливаю launcher.json + .sig в релиз $TAG…"
gh release upload "$TAG" "$WORK/launcher.json" "$WORK/launcher.json.sig" --repo "$REPO" --clobber

echo "✓ Готово: launcher.json для $TAG опубликован."
