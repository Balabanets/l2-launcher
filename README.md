# L2 Interlude Launcher

Современный апдейтер/лаунчер для сервера **aCis 409 (Interlude)**. Загружает и
**чинит** официальную копию клиента, **всегда проверяет целостность** перед запуском.

- **Стек:** Tauri 2 (Rust) + React/Vite/Tailwind v4. Дизайн — единый с сайтом (золото, тёмное фэнтези).
- **Цель сборки:** Windows (`.exe`/MSI), релизы через GitHub Actions.

## Как работает защита (Слой 1)

1. **Подписанный манифест.** На сервере генератор считает SHA-256 всех файлов клиента,
   подписывает `manifest.json` ключом **Ed25519**. Публичный ключ **вшит** в лаунчер
   (`src-tauri/src/manifest.rs`) → подменить манифест на раздаче невозможно.
2. **Проверка перед запуском.** Команда `play` всегда хеширует **критичные файлы**
   (`system/*.dll`, `*.exe`, `*.int`, …). Несовпадение → запуск блокируется, доступно
   «Восстановить».
3. **Запуск только из лаунчера.** Игру стартует сам лаунчер после успешной проверки.

**Слой 2 (позже, в aCis):** лаунчер шлёт на `/api/launcher/session` дайджест критичных
файлов и получает токен; игровой сервер при входе требует валидный токен → запуск мимо
лаунчера/с битыми файлами не проходит.

## Структура

```
crates/manifest      общие типы манифеста + подпись/проверка Ed25519 (+ тесты)
tools/manifest-gen   CLI: скан клиента → SHA-256 → подписанный manifest.json (+ keygen)
src-tauri/src        backend: manifest, scan, download, verify, launch, session, lib
src/                 frontend: App.tsx, components, lib/api.ts
.github/workflows    release.yml — Windows-сборка
```

## Ключи (хранятся вне репозитория!)

`~/.config/l2-launcher/keys/`:
- `manifest.key` / `manifest.pub` — Ed25519 для подписи манифеста (приват — секрет).
- `updater.key` / `updater.key.pub` — ключ автообновления Tauri.

Сгенерировать заново:
```bash
cargo run -p manifest-gen --bin keygen -- ~/.config/l2-launcher/keys/manifest   # подпись манифеста
npm run tauri signer generate -- -w ~/.config/l2-launcher/keys/updater.key       # апдейтер
```
Публичный ключ манифеста вшит в `src-tauri/src/manifest.rs` (`MANIFEST_PUBKEY`).
Публичный ключ апдейтера — в `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).

## Сборка / разработка

```bash
npm install
npm run tauri dev      # запуск в dev (Linux — для проверки UI/логики)
```
Релиз под Windows — пуш тега `vX.Y.Z` → GitHub Actions (нужны секреты репозитория
`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).

## Публикация клиента в R2

1. Эталон клиента: `/home/creative/l2-client-master/`.
2. Сгенерировать манифест:
   ```bash
   cargo run -p manifest-gen --bin manifest-gen -- \
     --client /home/creative/l2-client-master --out ./dist-manifest \
     --base-url https://cdn.l2.balabanets.uk/client/ --version $(date +%Y.%m.%d) \
     --key ~/.config/l2-launcher/keys/manifest.key
   ```
3. Залить в R2 (через `aws s3 --endpoint-url` или `wrangler`): файлы клиента в `client/`,
   плюс `manifest.json` и `manifest.json.sig` рядом.
4. В лаунчере `manifest_url` указывает на `https://cdn.l2.balabanets.uk/client/manifest.json`.

## Команды backend (Tauri invoke)

| Команда | Назначение |
|---|---|
| `get_config` / `save_config` | чтение/запись настроек |
| `check_update` | быстрая проверка (наличие+размер) |
| `start_update` | докачка изменённого (прогресс `update:progress`) |
| `repair` | полная проверка SHA-256 + починка |
| `verify_files` | полная проверка без скачивания |
| `play` | проверка критичных → токен → запуск |
