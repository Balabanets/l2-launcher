# L2 Interlude — Launcher & Client Distribution

Центральный документ проекта. Описывает **актуальный** сетап лаунчера, раздачи клиента и
связки с бэкендом. Если что-то меняется в архитектуре/хостах/релизах — обновляй этот файл и
раздел «История обновлений» внизу.

> Дата актуализации: 2026-07-11. Текущий релиз лаунчера: **v0.5.3**. Версия манифеста клиента:
> **2026.07.11.0138** (1544 файла).
>
> **⚠️ Раздача клиента переехала на Cloudflare R2.** Файлы клиента раздаёт **бакет R2
> `master-client`** через custom domain `l2files.balabanets.uk` — **напрямую с R2, без сервера**.
> nginx-контейнер `l2-cdn` и `l2files`-ingress тоннеля **выведены** (сервер — домашний интернет,
> раздача с него запрещена). Заливка нового клиента — `rclone` в R2 (см. §7).

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
GitHub Release                   https://l2files.balabanets.uk/c/<ver>/<путь>
Balabanets/l2-client             (Cloudflare R2 бакет master-client, custom domain —
  тег `manifest`:                 НАПРЯМУЮ с R2, сервер не участвует)
  manifest.json + .sig            ключи объектов: c/<ver>/<путь> (+ <путь>.zst)

   Слой 2 (онлайн-авторизация IP):
   лаунчер ──challenge/authorize──▶ https://l2.balabanets.uk/api/launcher/*
                                     (l2site, Next.js + Postgres)
                                     игровой сервер aCis ──check──▶ тот же бэкенд
```

- **Манифест** лежит на GitHub (`l2-client`, релиз `manifest`) — лаунчер берёт его оттуда
  (`config.rs` → `manifest_url`). Подпись проверяется **вшитым** в бинарь публичным ключом →
  подменить манифест нельзя.
- **Файлы клиента** раздаёт `l2files.balabanets.uk` — это **custom domain бакета R2
  `master-client`** (раздача напрямую с Cloudflare R2, сервер не участвует). Адресация **по путям**
  (`layout: "path"` → URL = `base_url` + относительный путь; ключ объекта в R2 = `c/<ver>/<путь>`).
  Range/resume поддержан R2 нативно. `.zst` лежат рядом как `<путь>.zst`.
- **Самообновление** лаунчера: `launcher.json` (+ `.sig`) и `latest.json` в релизе лаунчера.

## 3. Репозитории и хосты

| Что | Где |
|---|---|
| Репо лаунчера | `github.com/Balabanets/l2-launcher` (этот) |
| Релизы лаунчера (installer/portable/updater) | `l2-launcher` releases, тег `vX.Y.Z` |
| Репо клиента (только манифест) | `github.com/Balabanets/l2-client`, релиз `manifest` |
| Раздача файлов клиента | `https://l2files.balabanets.uk/c/<ver>/` → **R2 бакет `master-client`** (custom domain) |
| Бэкенд (Слой 2, статус, аккаунты, тикеты) | `https://l2.balabanets.uk` (проект `l2site`) |
| Игровой сервер | aCis 409 (Interlude) |

### Сервисы на сервере (Docker / systemd)
| Сервис | Назначение |
|---|---|
| ~~`l2-cdn` (nginx)~~ | **ВЫВЕДЕН** — раздача клиента переехала на R2 (см. §7). Контейнер удалён, `deploy/cdn.conf` оставлен для истории. |
| `l2site` (docker, `127.0.0.1:8090`) + `l2site-postgres` | сайт + бэкенд (auth, game-accounts, tickets, launcher Слой-2, статус) |
| `cloudflared-lineage2` (systemd) | тоннель: **только `l2.balabanets.uk`→8090**. `l2files`-ingress убран (домен теперь на R2). Конфиг `/etc/cloudflared/lineage2.yml` |

## 4. Структура кода

```
crates/manifest/         общий крейт: типы манифеста, подпись/проверка Ed25519, классы (groups.rs), safe_join
tools/manifest-gen/      CLI: скан клиента → SHA-256 + классы → подписанный manifest.json (+ bin keygen)
tools/publish-launcher.sh публикация launcher.json (метаданные самообновления) в релиз лаунчера
tools/publish.sh         R2-заливка через rclone (АКТУАЛЬНО снова — раздача на R2). См. §7 про no_check_bucket
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
- **R2-раздача (заливка клиента):** S3-ключ с правами **Object Read & Write** на бакет
  `master-client` — в rclone-remote `r2:` (`~/.config/rclone/rclone.conf`, нужен
  `no_check_bucket = true`). Рабочие R2-креды сайта — в `l2site/.env` (`R2_ACCESS_KEY_ID` и т.д.,
  бакет тикетов `l2-tickets`).
- **CF API-токен** (привязка `l2files`→R2, DNS): `~/.config/l2-launcher/keys/cf_api_token` (600),
  права Zone DNS:Edit + Workers R2 Storage:Edit. Токен «claude» (user-owned, полные права R2/DNS).
- **Роллить токены не нужно** — это тестовая среда (подтверждено владельцем 2026-07-11). Ключи R2/CF
  фигурировали в переписке, но ротация не требуется.
- **Тикеты (l2site) → бакет `l2-tickets`**, не путать с `master-client`. Конфиг R2 сайта в
  `l2site/.env`: `R2_BUCKET=l2-tickets`, `R2_ENDPOINT=https://0f5793a6…r2.cloudflarestorage.com`
  (2026-07-11 исправлен баг: было `Tickets` + пустой endpoint-плейсхолдер → загрузка вложений не
  работала). Код: `l2site/src/lib/r2.ts` (presigned PUT/GET, region `auto`).

### Kill-switch старых версий лаунчера (мин. версия)
Лаунчер шлёт `User-Agent: L2Launcher/<версия>`. Бэкенд Слоя 2 гейтит `authorize` по
`MIN_LAUNCHER_VERSION` (`l2site/.env`): версии ниже (и без `L2Launcher`-UA) → **426**, IP не
авторизуется, aCis не пускает → старые версии «мертвы». Сейчас `MIN_LAUNCHER_VERSION=0.6.9`.
Чтобы сделать новый релиз обязательным: поднять порог + `docker compose up -d --force-recreate l2site`
(env-file перечитывается только при пересоздании контейнера, не при `restart`). Самообновление
общее (`launcher.json`/updater-ключ) — не killable выборочно; заблокированные старые
самообновляются до текущей за ≤2 мин (self-healing). Код: `launcher-auth.ts:launcherVersionAllowed`,
`api/launcher/authorize/route.ts`.

**Модель защиты:**
- **Слой 1 (клиент):** подписанный манифест (подмена невозможна) + обязательная проверка
  критичных файлов перед запуском (`verify.rs`); игру запускает только лаунчер; анти-traversal.
- **Слой 2 (онлайн, сервер):** лаунчер берёт `challenge` (nonce), считает HMAC-отчёт реальных
  хешей критичных файлов с общим секретом (`LAUNCHER_HMAC_SECRET`), шлёт `authorize` → бэкенд
  сверяет с эталонным манифестом и авторизует IP. aCis при логине дёргает `check` (fail-closed).
  Heartbeat держит авторизацию всю сессию и ловит подмену в рантайме.

## 7. Операции (рабочие команды)

### Обновить/перезалить клиент (раздача на R2)
```bash
cd ~/Документы/my-projects/l2-launcher
# 0. master-client/ здесь — ЛОКАЛЬНЫЙ build-reference (НЕ раздаётся). Если пришёл «сырой» клиент
#    без _launcher/ — перенести _launcher/ (groups/perf/defaults) из текущего master-client и
#    убрать excluded: system/d3d8.dll, system/dgVoodoo.conf (perf-режимом владеет лаунчер).
#    Почистить состояние игрока: Cache/ Save/ system/AutoLogin*.ini s_info.ini _clip.log
# 1. ПОДПИСАННЫЙ + zstd манифест. base_url с версией в пути (/c/<ver>/) = immutable, свежий кэш.
VER=$(date +%Y.%m.%d.%H%M)
cargo run --release -p manifest-gen --bin manifest-gen -- \
  --client master-client --out dist-manifest \
  --base-url https://l2files.balabanets.uk/c/$VER/ --layout path \
  --version $VER --exe system/L2.exe --compress zstd \
  --key ~/.config/l2-launcher/keys/manifest.key
# 2. Список того, что реально качает лаунчер (для файла — .zst если comp=zstd, иначе сам файл):
python3 - <<'PY'
import json
m=json.load(open('dist-manifest/manifest.json'))
open('/tmp/upload-list.txt','w').write('\n'.join(
    f['path']+'.zst' if f.get('comp')=='zstd' else f['path'] for f in m['files'])+'\n')
PY
# 3. Залить в R2 бакет master-client под c/$VER/ (rclone remote r2:, ключи — см. §6).
#    ⚠️ ЛОВУШКА: у R2 нет GetBucketLocation → БЕЗ no_check_bucket=true rclone падает с 403
#    на предполётной проверке бакета. Поставь один раз: rclone config update r2 no_check_bucket true
rclone copy master-client "r2:master-client/c/$VER/" --files-from /tmp/upload-list.txt \
  --transfers 4 --checkers 8 --retries 5 --low-level-retries 20 --stats 30s
rclone check master-client "r2:master-client/c/$VER/" --files-from /tmp/upload-list.txt --size-only
# 4. Опубликовать манифест (лаунчер берёт его ОТСЮДА, файлы — с R2):
gh release upload manifest dist-manifest/manifest.json dist-manifest/manifest.json.sig \
  --repo Balabanets/l2-client --clobber
# 5. Проверка: живой манифест + отдача файла с R2 (cf-cache: DYNAMIC/HIT, 200; Range → 206).
curl -sL https://github.com/Balabanets/l2-client/releases/download/manifest/manifest.json | grep -o '"version":"[^"]*"'
curl -sI "https://l2files.balabanets.uk/c/$VER/system/L2.exe.zst" | grep -iE 'HTTP/|cf-cache'
# 6. (опц.) удалить прошлый префикс: rclone purge r2:master-client/c/<старый_VER>
# .zst создаются рядом с файлами при генерации (переиспользуются по mtime). Заливка ≈ 2.65 ГБ.
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

### Раздача = R2 custom domain (как связан `l2files` с бакетом)
Домен `l2files.balabanets.uk` привязан как **Custom Domain** к бакету R2 `master-client`
(Cloudflare сам держит proxied-CNAME `l2files → public.r2.dev` + edge-cert). Путь URL = ключ
объекта: `l2files.../c/<ver>/<путь>` ↔ объект `c/<ver>/<путь>` в бакете.

Управление через CF API (токен: **Zone `balabanets.uk` DNS:Edit + Account Workers R2 Storage:Edit**,
хранится в `~/.config/l2-launcher/keys/cf_api_token`, права 600). Account ID `0f5793a6…`:
```bash
CF=$(cat ~/.config/l2-launcher/keys/cf_api_token); AID=0f5793a6564912f465a86f39c8752c15
# статус привязки (ssl/ownership должны быть active):
curl -s -H "Authorization: Bearer $CF" \
  "https://api.cloudflare.com/client/v4/accounts/$AID/r2/buckets/master-client/domains/custom"
# заново привязать (если слетело): удалить конфликтный DNS l2files, затем POST домена:
#   POST .../r2/buckets/master-client/domains/custom  {"domain":"l2files.balabanets.uk","zoneId":"35f5d5b6…","enabled":true}
#   активация ssl/ownership занимает 1–5 мин.
```
> ⚠️ Раздачу с сервера (nginx `l2-cdn` + `l2files`-ingress тоннеля) **не поднимать** — это домашний
> интернет, канал раздачами грузить нельзя. Только R2.

### Cloudflare-кэш
На зоне есть Cache Rule для `l2files.balabanets.uk`: **cache 2xx (edge TTL 30д), 4xx/5xx → TTL 0**.
Версионированный путь `/c/<ver>/` гарантирует свежесть кэша на каждый релиз (immutable-ключи).

## 8. Известные нюансы
- Бэкап исходного архива клиента: `L2_Client_Release.zip` (~3.3 ГБ, gitignored) — держим как бэкап.
- `l2files.balabanets.uk` — **одноуровневый** поддомен намеренно: двухуровневый (`cdn.l2…`) не
  покрывается Universal SSL Cloudflare.
- Cloudflare-кэш иногда отдаёт старый 404 на новый путь несколько минут — не пугаться.
- **Раздача — только с R2** (бакет `master-client`, custom domain `l2files`). Сервер (домашний
  интернет) для раздачи не использовать. `master-client/` в репо-папке — локальный build-reference
  для генерации манифеста, **не раздаётся** (gitignored).
- `master-client/` (R2) — только файлы клиента; вложения тикетов сайта — отдельный бакет
  `l2-tickets`. Не путать.

---

## История обновлений

### Инфраструктура раздачи (эволюция)
- **R2 (план)** → не активирован (нужна привязка карты).
- **l2cdn.balabanets.uk** (тоннель+nginx) → отказ: двухуровневый поддомен не покрыт SSL.
- **CAS-шарды на GitHub Releases** (`client`/`client-2`, layout cas-multi) → отказ, релизы удалены.
- **l2files + nginx-контейнер `l2-cdn`** (тоннель → раздача `master-client` с сервера) → работало
  до 2026-07-11, затем выведено (нагрузка на домашний канал).
- **l2files → R2 бакет `master-client` (custom domain, layout: path)** ← **ТЕКУЩЕЕ (2026-07-11)**:
  файлы клиента раздаются напрямую с Cloudflare R2, сервер не участвует; манифест на GitHub.

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

### Инфра-миграция 2026-07-11 — раздача переехала на R2
- Новый клиент (сырой, с GDrive) распакован, перенесён `_launcher/`, вычищены excluded/состояние
  игрока → манифест **`2026.07.11.0138`** (1544 файла, zstd-раздача 2.65 ГБ), подпись сверена с
  вшитым `MANIFEST_PUBKEY`.
- Клиент залит в **R2 бакет `master-client` под `c/2026.07.11.0138/`** (rclone, `no_check_bucket`),
  сверка 1544/1544 + spot-check sha256 — ок.
- `l2files.balabanets.uk` переключён с тоннеля (nginx) на **custom domain R2** (удалён
  CNAME→тоннель, пересоздан R2-домен, ssl/ownership active). Отдача проверена: 200, Range 206,
  hash совпал.
- **Выведено:** контейнер `l2-cdn` удалён, `l2files`-ingress убран из тоннеля (остался только
  `l2.balabanets.uk`). Старая серверная копия `master-client` заменена build-reference; орфан
  R2-префикс `client/` удалён. Раздача с домашнего сервера прекращена.
