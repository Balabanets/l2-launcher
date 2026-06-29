# Antivirus False Positive — L2.exe (Windows Defender)

## Суть проблемы
`system/L2.exe` — подлинный модифицированный клиент Lineage 2 Interlude. Windows Defender
блокирует его **эвристически** (имена вида `Trojan:Win32/Wacatac`, `Gen:Variant`,
`ML.Attribute.HighConfidence`). Это **ложное срабатывание**, не реальный детект.

### Почему срабатывает (подтверждено статическим анализом)
- exe **не подписан** (нет Authenticode) → ML-модель Defender понижает доверие.
- Импортирует `MinHook.x86.dll` (перехват WinAPI) — легитимная клиентская защита L2,
  но та же техника встречается в малвари → эвристика реагирует.
- Встроенные крипто-кодеки `FL2DESCodec`/`FL2RSACodec` (распаковка архивов клиента) —
  ML коррелирует «крипто + хукинг + неподписан» с пакерами.

### Чего в файле НЕТ (почему это точно FP)
- Нет инъекций в чужие процессы (`WriteProcessMemory`/`CreateRemoteThread`/`SetWindowsHookEx`).
- Нет зашитых URL/IP/C2-доменов, нет сетевого back-connect.
- Нет персистентности (ключи Run / schtasks / службы).
- Присутствует оригинальное дерево сборки Interlude и подлинные движковые символы.

## Хеши (для подачи и для верификации)
```
SHA-256: 6cc91c3e09f5330861d16e42608cd2b3e27ee97a3d740bfed97ea334c8e034eb
MD5:     b749c201a3e90aa35d27f3ce896d5c9c
Размер:  507904 байт
```

## Как подать в Microsoft (бесплатно)
1. Открыть https://www.microsoft.com/en-us/wdsi/filesubmission
2. Войти под Microsoft-аккаунтом → **Submit as: Software developer**
   (важно: «developer», а не «home customer» — у разработчиков выше приоритет).
3. Загрузить `system/L2.exe`.
4. Detection name — вписать ровно то, что показал Defender (см. карантин Defender →
   «История защиты»).
5. В поле обоснования вставить текст из блока ниже.
6. Отправить. Статус придёт на почту; при подтверждении Defender перестаёт блокировать
   у всех игроков с очередным обновлением сигнатур (часы–двое суток).

### Готовый текст обоснования (скопировать в форму, на английском)
```
This is the game client executable of a private Lineage 2 Interlude server that I operate
and distribute to my players. The detection is a false positive.

The binary is an Unreal Engine 2 game client. It is flagged heuristically because it is
unsigned and uses the MinHook API-hooking library for legitimate in-client protection,
plus embedded DES/RSA codecs to decrypt the game's own asset archives.

Static analysis shows no process injection (no WriteProcessMemory/CreateRemoteThread/
SetWindowsHookEx), no network C2 (no hardcoded URLs/IPs), and no persistence
(no Run keys/scheduled tasks/services). It contains the original Interlude engine build
artifacts and symbols. Please reclassify as clean / add to the allow list.
```

## После одобрения
- Defender снимает блок автоматически у всех — игрокам ничего делать не нужно.
- Файл, скачанный **лаунчером** (а не браузером), как правило не получает Mark-of-the-Web,
  поэтому SmartScreen по «скачано из интернета» к нему не цепляется.
- При пересборке L2.exe (меняется хеш) — подать заново. L2.exe статичен и меняется редко,
  так что одной подачи обычно достаточно надолго.

## Бесплатный долгосрочный вариант: code signing через SignPath OSS
Если репозиторий лаунчера сделать публичным (open source), можно получить **бесплатный**
Authenticode-сертификат для OSS-проектов через SignPath Foundation (https://signpath.io/).
Тогда подписываются и лаунчер, и L2.exe → будущие сборки не триггерят эвристику и SmartScreen
успокаивается. Требует: публичный репозиторий + одобрение фонда + настройка CI.
