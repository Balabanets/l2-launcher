manifest-gen — генератор подписанного манифеста клиента L2

Обязательные флаги:
  --client   <путь>   корень папки клиента
  --out      <путь>   куда писать manifest.json + .sig (по умолчанию ./dist)
  --base-url <url>    базовый URL раздачи (R2), например https://cdn.l2.balabanets.uk/client/
  --version  <строка> версия набора, например 2026.06.06
  --key      <файл>   приватный ключ Ed25519 (32 байта, hex или base64)

Необязательные:
  --critical <glob>   паттерн критичного файла (можно несколько раз)
  --exe      <путь>   исполняемый файл запуска (по умолчанию system/l2.exe)
  --cwd      <путь>   рабочая директория (по умолчанию system)

Пример:
  manifest-gen --client /home/creative/l2-client-master --out ./dist \
    --base-url https://cdn.l2.balabanets.uk/client/ --version 2026.06.06 \
    --key ./keys/manifest_ed25519.key
