# L2 Interlude — Launcher & Client Distribution

Центральный документ проекта. Описывает **актуальный** сетап лаунчера, раздачи клиента и
связки с бэкендом. Если что-то меняется в архитектуре/хостах/релизах — обновляй этот файл и
раздел «История обновлений» внизу.

> Дата актуализации: 2026-06-12. Текущий релиз лаунчера: **v0.4.2**. Версия манифеста клиента:
> **2026.06.12.1959** (1519 файлов).

---

## 1. Что это

Современный апдейтер/лаунчер для приватного сервера **Lineage 2 Interlude (aCis 409)**:
загружает и **чинит** официальную копию клиента, **всегда проверяет целостность** перед
запуском, держит онлайн-авторизацию игрока. Дизайн — единый с сайтом (тёмное фэнтези, золото).

- **Стек:** Tauri 2 (Rust) + React/Vite/Tailwind v4. Сборка — только **Windows**, релизы через GitHub Actions.

## 2. Архитектура (как это работает сейчас)

```
                 ┌─────────────────────────────────────────────┐
                 │  Лаунчер (Tauri, у игрока, Windows)          │
                 └───────────────┬─────────────────────────────┘
   манифест (подписан Ed25519)   │   файлы клиента (layout: path)
   ▼                              ▼
GitHub Release                   https://l2files.balabanets.uk/client/<путь>
Balabanets/l2-client             (nginx-контейнер l2-cdn :8091 ← тоннель lineage2)
  тег `manifest`:                раздаёт ./master-client как /client/
  manifest.json + .sig

   Слой 2 (онлайн-авторизация IP):
   лаунчер ──challenge/authorize──▶ https://l2.balabanets.uk/api/launcher/*
                                     (l2site, Next.js + Postgres)
                                     игровой сервер aCis ──check──▶ тот же бэкенд
```

- **Манифест** лежит на GitHub (`l2-client`, релиз `manifest`) — лаунчер берёт его оттуда
  (`config.rs` → `manifest_url`). Подпись проверяется **вшитым** в бинарь публичным ключом →
  подменить манифест нельзя.
- **Файлы клиента** раздаёт `l2files.balabanets.uk` (nginx через тоннель), адресация **по путям**
  (`layout: "path"` → URL = `base_url` + относительный путь). Поддержка Range/resume.
- **Самообновление** лаунчера: `launcher.json` (+ `.sig`) и `latest.json` в релизе лаунчера.

## 3. Репозитории и хосты

| Что | Где |
|---|---|
| Репо лаунчера | `github.com/Balabanets/l2-launcher` (этот) |
| Релизы лаунчера (installer/portable/updater) | `l2-launcher` releases, тег `vX.Y.Z` |
| Репо клиента (только манифест) | `github.com/Balabanets/l2-client`, релиз `manifest` |
| Раздача файлов клиента | `https://l2files.balabanets.uk/client/` |
| Бэкенд (Слой 2, статус, аккаунты, тикеты) | `https://l2.balabanets.uk` (проект `l2site`) |
| Игровой сервер | aCis 409 (Interlude) |

### Сервисы на сервере (Docker / systemd)
| Сервис | Назначение |
|---|---|
| `l2-cdn` (docker, nginx:alpine, `127.0.0.1:8091`) | раздаёт `./master-client` как `/client/`; `restart: unless-stopped` |
| `l2site` (docker, `127.0.0.1:8090`) + `l2site-postgres` | сайт + бэкенд (auth, game-accounts, tickets, launcher Слой-2, статус) |
| `cloudflared-lineage2` (systemd) | тоннель: `l2.balabanets.uk`→8090, `l2files.balabanets.uk`→8091. Конфиг `/etc/cloudflared/lineage2.yml` |

## 4. Структура кода

```
crates/manifest/         общий крейт: типы манифеста, подпись/проверка Ed25519, классы (groups.rs), safe_join
tools/manifest-gen/      CLI: скан клиента → SHA-256 + классы → подписанный manifest.json (+ bin keygen)
tools/publish-launcher.sh публикация launcher.json (метаданные самообновления) в релиз лаунчера
tools/publish.sh         СТАРЫЙ R2-вариант раздачи — НЕ используется (оставлен для истории)
src-tauri/src/           backend (Rust):
  config.rs              настройки (install_dir, manifest_url, api_base, сервер); валидация URL
  manifest.rs            загрузка манифеста + проверка подписи (вшитый MANIFEST_PUBKEY) + анти-traversal
  scan.rs                скан файлов, дельта (Quick=размер / Hash=SHA-256)
  download.rs            параллельная докачка с resume, пер-файловая проверка хеша
  sync.rs                КЛАССОВАЯ синхронизация (managed/optional-языки/seed-once/launcher-owned)
  verify.rs              обязательная проверка критичных файлов перед запуском
  launch.rs              запуск игры (+ авто-эскалация прав через UAC при os error 740)
  session.rs             Слой 2: challenge → HMAC-отчёт → authorize (онлайн-авторизация IP)
  selfupdate.rs          самообновление лаунчера (проверка launcher.json подписи, релонч)
  client_settings.rs     настройки клиента: режим производительности (perf) + язык RU/EN
  control.rs / progress.rs  отмена операций / события прогресса в UI
src/                     frontend (React): App.tsx, components, lib/api.ts
.github/workflows/release.yml   Windows-сборка по тегу vX.Y.Z
master-client/           ЭТАЛОННАЯ копия клиента (~7 ГБ, gitignored) — раздаётся nginx-ом
```

## 5. Классы файлов клиента (`master-client/_launcher/groups/*.txt`)

`manifest-gen` классифицирует каждый файл; лаунчер (`sync.rs`) ведёт себя по классу:

| Класс | Список | Поведение лаунчера |
|---|---|---|
| **managed** | всё остальное | хэш-синк: качать/чинить по манифесту |
| **optional** | `lang-ru.txt`, `lang-en.txt` | языковые наборы; RU всегда, EN — по флагу/если скачан |
| **seed-once** | `seed-once.txt` (user.ini, Option.ini, chatfilter.ini) | писать ТОЛЬКО если отсутствует; апдейт не перезатирает |
| **launcher-owned** | `launcher-owned.txt` (l2.ini, Localization.ini) | состоянием владеет лаунчер (perf/язык), переприменяет после апдейта |
| **excluded** | `preserve.txt` + d3d8/dgVoodoo/WindowsInfo.ini | не в манифесте (состояние игрока / выбирается perf-режимом) |

`_launcher/` также везёт `perf/` (варианты d3d8 для perf-режима) и `defaults/` (WindowsInfo) —
лаунчер берёт их как источники при применении настроек.

## 6. Ключи и безопасность

Ключи **вне репозитория**: `~/.config/l2-launcher/keys/`
- `manifest.key`/`.pub` — Ed25519 подписи манифеста **и** `launcher.json`. Pubkey **вшит** в
  `src-tauri/src/manifest.rs` (`MANIFEST_PUBKEY`).
- `updater.key`/`.pub` — ключ Tauri-updater (для `latest.json`). Pubkey в `tauri.conf.json`.
- CI-секреты репо: `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`).

**Модель защиты:**
- **Слой 1 (клиент):** подписанный манифест (подмена невозможна) + обязательная проверка
  критичных файлов перед запуском (`verify.rs`); игру запускает только лаунчер; анти-traversal.
- **Слой 2 (онлайн, сервер):** лаунчер берёт `challenge` (nonce), считает HMAC-отчёт реальных
  хешей критичных файлов с общим секретом (`LAUNCHER_HMAC_SECRET`), шлёт `authorize` → бэкенд
  сверяет с эталонным манифестом и авторизует IP. aCis при логине дёргает `check` (fail-closed).
  Heartbeat держит авторизацию всю сессию и ловит подмену в рантайме.

## 7. Операции (рабочие команды)

### Обновить/перезалить клиент
```bash
cd ~/Документы/my-projects/l2-launcher
# 1. Положить новый клиент в ./master-client (system/L2.exe в корне, есть _launcher/groups)
#    Почистить состояние игрока: Cache/ Save/ system/AutoLogin*.ini s_info.ini _clip.log
# 2. Сгенерировать подписанный манифест:
cargo run --release -p manifest-gen --bin manifest-gen -- \
  --client master-client --out dist-manifest \
  --base-url https://l2files.balabanets.uk/client/ --layout path \
  --version $(date +%Y.%m.%d.%H%M) --exe system/L2.exe \
  --key ~/.config/l2-launcher/keys/manifest.key
cp dist-manifest/manifest.json dist-manifest/manifest.json.sig master-client/   # для l2files
# 3. Опубликовать манифест (лаунчер берёт его отсюда):
gh release upload manifest dist-manifest/manifest.json dist-manifest/manifest.json.sig \
  --repo Balabanets/l2-client --clobber
# nginx уже раздаёт master-client → новые файлы доступны сразу.
```

### Выпустить новую версию лаунчера
```bash
# бамп версии в package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml
git commit -am "..."; git push
git tag vX.Y.Z && git push origin vX.Y.Z      # → CI собирает Windows-релиз
tools/publish-launcher.sh vX.Y.Z              # подписать+залить launcher.json (самообновление)
```

### Раздача (если nginx/тоннель надо пересоздать)
```bash
docker run -d --name l2-cdn --restart unless-stopped -p 127.0.0.1:8091:80 \
  -v "$PWD/master-client":/usr/share/nginx/html/client:ro nginx:alpine
# ingress l2files.balabanets.uk → 127.0.0.1:8091 в /etc/cloudflared/lineage2.yml (через sudo)
```

## 8. Известные нюансы
- Бэкап исходного архива клиента: `L2_Client_Release.zip` (~3.3 ГБ, gitignored) — держим как бэкап.
- `l2files.balabanets.uk` — **одноуровневый** поддомен намеренно: двухуровневый (`cdn.l2…`) не
  покрывается Universal SSL Cloudflare.
- Cloudflare-кэш иногда отдаёт старый 404 на новый путь несколько минут — не пугаться.
- Раздача клиента через тоннель — это **временное/малое масштабирование**. При росте онлайна
  перенести файлы в R2/CDN и поменять `base_url` в манифесте (лаунчер не трогать).

---

## История обновлений

### Инфраструктура раздачи (эволюция)
- **R2 (план)** → не активирован (нужна привязка карты).
- **l2cdn.balabanets.uk** (тоннель+nginx) → отказ: двухуровневый поддомен не покрыт SSL.
- **CAS-шарды на GitHub Releases** (`client`/`client-2`, layout cas-multi) → отказ, релизы удалены.
- **l2files.balabanets.uk (layout: path)** ← **текущее**: nginx раздаёт `master-client`, манифест на GitHub.

### Версии лаунчера
- **v0.1.0** — первый лаунчер: Слой 1 (подписанный манифест, скан/докачка/verify, запуск),
  CI Windows-сборка, интеграционные тесты.
- **v0.2.x** — CAS-адресация файлов, отмена загрузок, cas-multi с фолбэком источников.
- **v0.3.x** — Слой 2 (HMAC онлайн-авторизация IP: challenge/authorize/check), heartbeat
  переавторизации + рантайм-детект подмены, запуск с правами админа (UAC), самообновление,
  живой дизайн + карточки/онлайн-статус серверов, прогресс проверки перед запуском.
- **v0.4.0–0.4.1** — portable-сборка + portable self-update; настройки клиента (perf-режим,
  язык RU/EN); карточки серверов в стиле сайта.
- **v0.4.2 (текущая)** — классовая синхронизация (`groups.rs`/`sync.rs`): optional-языки по
  требованию, seed-once (не затирать настройки игрока), launcher-owned (perf/язык). Залит новый
  клиент (manifest 2026.06.12.1959, 1519 файлов), раздача переведена на l2files (path),
  старые CAS-релизы удалены.
