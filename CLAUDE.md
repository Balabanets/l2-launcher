# L2 Interlude — Launcher & Client Distribution

Центральный документ проекта. Описывает **актуальный** сетап лаунчера, раздачи клиента и
связки с бэкендом. Если что-то меняется в архитектуре/хостах/релизах — обновляй этот файл и
раздел «История обновлений» внизу.

> Дата актуализации: 2026-06-29. Текущий релиз лаунчера: **v0.5.3**. Версия манифеста клиента:
> **2026.06.14.1441** (1551 файл).

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
# 2. Сгенерировать ПОДПИСАННЫЙ + СЖАТЫЙ (zstd) манифест. ВАЖНО: base_url с версией в пути
#    (/c/<ver>/) — это immutable-путь, свежий кэш-ключ Cloudflare (без залипших 404).
VER=$(date +%Y.%m.%d.%H%M)
cargo run --release -p manifest-gen --bin manifest-gen -- \
  --client master-client --out dist-manifest \
  --base-url https://l2files.balabanets.uk/c/$VER/ --layout path \
  --version $VER --exe system/L2.exe --compress zstd \
  --key ~/.config/l2-launcher/keys/manifest.key
cp dist-manifest/manifest.json dist-manifest/manifest.json.sig master-client/
# 3. Опубликовать манифест (лаунчер берёт его отсюда):
gh release upload manifest dist-manifest/manifest.json dist-manifest/manifest.json.sig \
  --repo Balabanets/l2-client --clobber
# nginx уже раздаёт master-client (+ .zst рядом) → новые файлы доступны сразу.
# .zst создаются рядом с файлами при --compress (переиспользуются по mtime). Загрузка ≈ 2.8 ГБ.
```

### Выпустить новую версию лаунчера — ЧЕК-ЛИСТ (по порядку)
```bash
# 1. Бамп версии в ТРЁХ местах + Cargo.lock:
#    package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml
cargo update -p l2-launcher --precise X.Y.Z
# 2. Сборка/тесты локально (фронт + воркспейс):
npm run build && cargo test --workspace
# 3. Коммит и пуш main:
git commit -am "feat: ..."; git push
# 4. Тег ДОЛЖЕН указывать на свежий HEAD (иначе CI соберёт не то):
git tag vX.Y.Z && git push origin vX.Y.Z      # → CI собирает Windows-релиз (~9 мин)
#    проверка: [ "$(git rev-list -n1 vX.Y.Z)" = "$(git rev-parse HEAD)" ]
# 5. ⚠️ ОБЯЗАТЕЛЬНО после зелёного CI — иначе самообновление НЕ увидит версию:
tools/publish-launcher.sh vX.Y.Z              # подписать+залить launcher.json
# 6. Проверить, что latest отдаёт новую версию (CDN может кэшировать пару секунд):
curl -sL .../releases/latest/download/launcher.json | grep version
```
> **Главная ловушка (спотыкались на v0.4.9):** CI собирает бинари и `latest.json`, но
> подписанный `launcher.json` (метаданные кастомного самообновления) заливает ТОЛЬКО шаг 5.
> Без него игроки не видят обновление. Ключ Ed25519 — локальный (в CI его нет намеренно),
> поэтому шаг ручной. Самообновление в открытом лаунчере опрашивает `launcher.json` раз в 2 мин.

### Раздача (если nginx/тоннель надо пересоздать)
```bash
# nginx с версионированным путём /c/<ver>/<file> → client/<file> (см. deploy/cdn.conf)
docker run -d --name l2-cdn --restart unless-stopped -p 127.0.0.1:8091:80 \
  -v "$PWD/master-client":/usr/share/nginx/html/client:ro \
  -v "$PWD/deploy/cdn.conf":/etc/nginx/conf.d/default.conf:ro nginx:alpine
# ingress l2files.balabanets.uk → 127.0.0.1:8091 в /etc/cloudflared/lineage2.yml (sudo)
# DNS l2files должен указывать на тоннель lineage2:
cloudflared tunnel route dns --overwrite-dns lineage2 l2files.balabanets.uk
```

### Cloudflare (ВАЖНО: зона кэширует «Cache Everything» через legacy Page Rule)
На зоне есть Page Rule, кэширующий всё (включая 404 → залипают). Поэтому стоит **Cache Rule**
(новый движок, приоритет над Page Rules) для `l2files.balabanets.uk`:
**cache 2xx (edge TTL 30д), 4xx/5xx → TTL 0 (не кэшировать)**. Это даёт CDN-оптимизацию и не
залипает на 404. Правило ставится через CF API (Zone: Cache Rules Edit + Cache Purge).
Версионированный путь `/c/<ver>/` дополнительно гарантирует свежесть кэша на каждый релиз.

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
- **v0.4.2** — классовая синхронизация (`groups.rs`/`sync.rs`): optional-языки по требованию,
  seed-once (не затирать настройки игрока), launcher-owned (perf/язык). Раздача на l2files (path),
  старые CAS-релизы удалены.
- **v0.4.3 (текущая)** — UX-фиксы по фидбеку: **zstd-сжатие раздачи** (загрузка 7.1→2.8 ГБ,
  `comp/csize` в манифесте, `.zst` рядом с файлами); **perf/язык — настройки** (мгновенно, без
  ошибок и без мелькающих окон; применяются тихо при Играть/Обновить); **portable встаёт в
  `%LOCALAPPDATA%` + ярлык** (`install.rs`, mslnk) — самообновление больше не ломает ярлык;
  **иконка .exe** (арт L2 Interlude). Инфра: версионированный путь раздачи `/c/<ver>/` +
  Cloudflare Cache Rule (cache 2xx, 4xx/5xx не кэшировать) — починен залипший 404-кэш зоны;
  DNS l2files перенаправлен на тоннель lineage2.
