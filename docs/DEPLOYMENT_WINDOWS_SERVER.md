# Развертывание на Windows Server - Пошаговая инструкция

## 📦 Что нужно для развертывания

### Минимальный набор файлов:

```
C:\Users\udv\Desktop\MPI\
├── backend.exe              (исполняемый файл)
├── config.toml              (конфигурация)
└── [папка data создастся автоматически]
```

### Опциональные файлы:

```
├── migrate_auth_system.sql  (только для первого запуска или обновлений БД)
├── dist\                     (статические файлы фронтенда, если используется)
```

---

## 🚀 Шаг 1: Подготовка файлов

### 1.1. Скопируйте backend.exe

Из папки разработки:

```
target\release\backend.exe  →  C:\Users\udv\Desktop\MPI\backend.exe
```

### 1.2. Создайте config.toml

Создайте файл `C:\Users\udv\Desktop\MPI\config.toml`. Расположение данных задаётся
**одной** настройкой — корнем `[data].root`; база и все каталоги выводятся из него
(`db/app.db`, `knowledge/`, `skills/`, `chats/`, `golden_set/`, `quality_checks/`,
`attachments/`, `backups/`, `tmp/` — создаются при старте).

**Вариант A: С прямыми слешами (рекомендуется)**

```toml
# Marketplace Integrator Configuration
[data]
root = "C:/Users/udv/Desktop/MPI/data"
```

**Вариант B: С одинарными кавычками (обратные слеши без экранирования)**

```toml
# Marketplace Integrator Configuration
[data]
root = 'C:\Users\udv\Desktop\MPI\data'
```

> Путь ОБЯЗАН быть абсолютным: относительный отвергается при старте, иначе данные
> «переезжают» вслед за рабочим каталогом процесса.
> Отдельный `[database].path` нужен, только если база обязана лежать вне корня
> (например, на другом диске) — он тоже обязан быть абсолютным.

Полный пример со всеми секциями — `config.toml.example` в репозитории.

### 1.3. (Опционально) Скопируйте файл миграций

Если это первый запуск или есть обновления БД:

```
migrate_auth_system.sql  →  C:\Users\udv\Desktop\MPI\migrate_auth_system.sql
```

**Если файла нет** - ничего страшного! Приложение выдаст предупреждение и продолжит работу.

---

## 🎯 Шаг 2: Первый запуск

### 2.1. Запустите backend.exe

Откройте PowerShell или Command Prompt:

```cmd
cd C:\Users\udv\Desktop\MPI
.\backend.exe
```

### 2.2. Проверьте вывод

Вы должны увидеть:

```
╔══════════════════════════════════════════════════════════╗
║           MARKETPLACE BACKEND STARTING...               ║
╚══════════════════════════════════════════════════════════╝

Step 1: Initializing logging system...
✓ Logging system initialized

Step 2: Initializing database...
✓ Configuration loaded successfully!
✓ Database initialized successfully

Step 3: Checking for authentication system migrations...
[либо миграция выполнена, либо предупреждение]
✓ Auth migrations processed

...

╔══════════════════════════════════════════════════════════╗
║           SERVER STARTED SUCCESSFULLY!                  ║
║  Server listening on: http://0.0.0.0:3000              ║
╚══════════════════════════════════════════════════════════╝
```

### 2.3. Проверьте доступность

Откройте браузер:

- На самом сервере: `http://localhost:3000`
- С другого компьютера: `http://IP_сервера:3000`

---

## 🔧 Шаг 3: Настройка Windows

### 3.1. Открыть порт в файрволе (для удаленного доступа)

```powershell
# Запустите PowerShell от имени администратора
New-NetFirewallRule -DisplayName "Marketplace Backend" -Direction Inbound -LocalPort 3000 -Protocol TCP -Action Allow
```

### 3.2. Создать службу Windows (опционально)

Чтобы приложение запускалось автоматически:

#### Вариант A: Использовать планировщик заданий

1. Откройте **Планировщик заданий** (Task Scheduler)
2. Создайте задание:
   - **Имя:** Marketplace Backend
   - **Триггер:** При запуске системы
   - **Действие:** Запустить программу `C:\Users\udv\Desktop\MPI\backend.exe`
   - **Рабочая папка:** `C:\Users\udv\Desktop\MPI`
   - **Запускать:** От имени администратора

#### Вариант B: Использовать NSSM (рекомендуется)

1. Скачайте NSSM: https://nssm.cc/download
2. Установите службу:

```cmd
nssm install MarketplaceBackend "C:\Users\udv\Desktop\MPI\backend.exe"
nssm set MarketplaceBackend AppDirectory "C:\Users\udv\Desktop\MPI"
nssm set MarketplaceBackend AppExit Default Restart
nssm set MarketplaceBackend AppRestartDelay 2000
nssm start MarketplaceBackend
```

`AppExit Default Restart` обязателен для автоматического применения восстановленной
БД: после подготовки `pending_restore` backend штатно завершается, NSSM запускает
его снова, и подмена выполняется до открытия пула подключений. Вариант с обычным
заданием «При запуске системы» этого не умеет.

---

## 📊 Шаг 4: Мониторинг и логи

### 4.1. Логи приложения

Логи автоматически записываются в:

```
C:\Users\udv\Desktop\MPI\logs\backend.log
```

### 4.2. Просмотр логов

```cmd
# Просмотр последних записей
type C:\Users\udv\Desktop\MPI\logs\backend.log | more

# Мониторинг в реальном времени (PowerShell)
Get-Content C:\Users\udv\Desktop\MPI\logs\backend.log -Wait -Tail 20
```

### 4.3. Проверка процесса

```cmd
# Проверить, запущен ли процесс
tasklist | findstr backend.exe

# Проверить, занят ли порт 3000
netstat -ano | findstr :3000
```

---

## 🛠️ Устранение неполадок

### Проблема: "Port 3000 is already in use"

**Решение:**

```cmd
# Найти процесс, занимающий порт
netstat -ano | findstr :3000

# Завершить процесс (замените PID на найденный)
taskkill /PID <PID> /F
```

### Проблема: "Cannot access file"

**Причины:**

- Антивирус блокирует доступ
- Недостаточно прав

**Решение:**

- Запустите от имени администратора
- Добавьте папку в исключения антивируса

### Проблема: "Invalid TOML format"

**Решение:**
Используйте одну из корректных форматов в config.toml (см. Шаг 1.2)

### Проблема: База данных заблокирована

**Причины:**

- Запущено несколько экземпляров backend.exe
- Другая программа использует БД

**Решение:**

```cmd
# Найти все процессы backend.exe
tasklist | findstr backend.exe

# Завершить все
taskkill /IM backend.exe /F
```

---

## 🔒 Безопасность

### При первом запуске создается администратор:

```
Username: admin
Password: admin
```

**⚠️ ВАЖНО: Немедленно смените пароль!**

### Рекомендации:

- ✅ Измените пароль admin
- ✅ Используйте HTTPS (настройте reverse proxy, например, nginx)
- ✅ Настройте файрволл
- ✅ Регулярно делайте backup БД (`data\app.db`)

---

## 📋 Структура папок после запуска

```
C:\Users\udv\Desktop\MPI\
├── backend.exe
├── config.toml
├── migrate_auth_system.sql (опционально)
├── data\
│   └── app.db (создастся автоматически)
└── logs\
    └── backend.log (создастся автоматически)
```

---

## 🔄 Обновление приложения

1. Остановите backend.exe
2. Замените backend.exe на новую версию
3. Скопируйте новый migrate_auth_system.sql (если есть)
4. Запустите backend.exe
5. Миграции применятся автоматически

**База данных и конфигурация сохраняются!**

---

## 📞 Поддержка

При возникновении проблем соберите следующую информацию:

1. **Полный вывод консоли при запуске**
2. **Содержимое config.toml**
3. **Последние строки из logs\backend.log**
4. **Версия Windows Server**
5. **Результат команды:**
   ```cmd
   backend.exe --version  # (если поддерживается)
   ```

---

## ✅ Чеклист развертывания

### Файлы:

- [ ] Скопирован backend.exe
- [ ] Создан config.toml с правильным путем
- [ ] **Скопирована папка dist/ (frontend)** ⚠️ ОБЯЗАТЕЛЬНО!

### Запуск:

- [ ] Приложение успешно запускается
- [ ] Логи пишутся в logs\backend.log
- [ ] База данных создалась в data\app.db

### Сеть:

- [ ] Узнан IP адрес сервера (`ipconfig`)
- [ ] Порт 3000 открыт в файрволе
- [ ] С самого сервера доступ работает (`http://localhost:3000`)
- [ ] **С другого компьютера доступ работает (`http://<IP>:3000`)** ⚠️ ВАЖНО!
- [ ] Frontend открывается через IP адрес, НЕ через file://

### Безопасность:

- [ ] Изменен пароль администратора (admin/admin → новый пароль)

### Опционально:

- [ ] Настроено автоматическое запуск (планировщик или NSSM)
- [ ] Настроен backup БД

---

## 🎉 Готово!

Приложение развернуто и готово к использованию!

Дополнительные материалы:

- **РЕШЕНИЕ_FAILED_TO_FETCH.md** - решение проблемы "Failed to fetch" при авторизации
- **FRONTEND_BACKEND_CONNECTION.md** - подробное руководство по подключению frontend к backend
- **РЕШЕНИЕ_ПРОБЛЕМЫ_V2.md** - решение конкретной проблемы с путями
- **CHANGELOG_DIAGNOSTICS.md** - технические детали изменений
- **DIAGNOSTICS_GUIDE.md** - руководство по диагностике
