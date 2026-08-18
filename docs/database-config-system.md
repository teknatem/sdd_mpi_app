# Система конфигурации базы данных

## Обзор

Реализована гибкая система конфигурации пути к базе данных через файл `config.toml`. Система решает проблему создания нескольких баз данных при запуске из разных директорий.

## Как это работает

### Структура конфигурационного файла

```toml
[database]
# Путь к файлу базы данных (относительный или абсолютный)
path = "../../target/db/app.db"
```

### Логика загрузки

1. **При компиляции** (build.rs):
   - Копирует `config.toml` из workspace root в `target/debug/` или `target/release/`
   - Выводит предупреждение если config.toml не найден

2. **При запуске программы** (config.rs):
   - Ищет `config.toml` рядом с исполняемым файлом `backend.exe`
   - Если найден - загружает и парсит конфигурацию
   - Если не найден - использует встроенный default: `"target/db/app.db"`

3. **Разрешение пути** (config.rs):
   - Абсолютный путь используется как есть
   - Относительный путь разрешается от директории с `backend.exe`

### Логирование

При запуске backend выводит:
```
INFO backend::shared::config: Loading config from: /path/to/config.toml
INFO backend::shared::data::db: Database path: /absolute/path/to/app.db
INFO backend::shared::data::db: Connecting to database: sqlite://...
```

Это позволяет легко диагностировать проблемы с путем к БД.

## Использование

### Development

```powershell
# 1. Отредактируйте config.toml в workspace root (при необходимости)
notepad config.toml

# 2. Запустите backend (из любой директории)
cargo run --bin backend

# Build script автоматически скопирует config.toml в target/debug/
```

**Текущая конфигурация для dev:**
```toml
[database]
path = "../../target/db/app.db"
```

Логика: `target/debug/backend.exe` → `../../target/db/app.db` → `workspace_root/target/db/app.db`

### Production

#### Вариант 1: Config рядом с exe

```powershell
# 1. Скомпилируйте release версию
cargo build --release --bin backend

# 2. Скопируйте файлы в целевую директорию
cp target/release/backend.exe C:/MyApp/
cp config.toml C:/MyApp/

# 3. Отредактируйте config.toml для production
cd C:/MyApp
notepad config.toml
```

**Пример production конфигурации:**
```toml
[database]
path = "C:/MyApp/data/production.db"
# или относительный:
# path = "./data/production.db"
```

#### Вариант 2: Запуск из workspace

```powershell
# Запустить release версию из workspace
./target/release/backend.exe
# Найдет config.toml в target/release/ (скопирован build script)
```

### Разные окружения (dev/test/prod)

**Способ 1: Разные config.toml**

```powershell
# Development
cp config.dev.toml config.toml
cargo run --bin backend

# Production
cp config.prod.toml target/release/config.toml
./target/release/backend.exe
```

**Способ 2: Разные пути в одном config.toml**

```toml
[database]
# Раскомментируйте нужный вариант:

# Development
path = "../../target/db/app.db"

# Test
# path = "../../target/db/test.db"

# Production
# path = "E:/Production/data/marketplace.db"
```

## Примеры конфигураций

### Относительный путь (от exe)

```toml
[database]
path = "../../target/db/app.db"  # Вверх на 2 уровня, затем в target/db
path = "./data/app.db"           # В подпапке data рядом с exe
path = "../shared/database.db"   # В родительской директории
```

### Абсолютный путь

```toml
[database]
# Windows
path = "F:/data/sdd_mpi_app/app.db"
path = "C:/Production/MarketplaceData/database.db"

# Linux/Mac
path = "/home/user/projects/marketplace/data/app.db"
path = "/var/lib/marketplace/database.db"
```

## Преимущества новой системы

1. **Надежность**: Не зависит от директории запуска (`current_dir()`)
2. **Гибкость**: Легко менять путь к БД без пересборки
3. **Прозрачность**: Логирование показывает используемый путь
4. **Удобство**: Автоматическое копирование конфига при сборке
5. **Fallback**: Работает даже если config.toml отсутствует
6. **Портативность**: Config рядом с exe для production

## Файлы изменены

- ✅ `crates/backend/Cargo.toml` - добавлена зависимость `toml`
- ✅ `crates/backend/build.rs` - создан build script для копирования config
- ✅ `crates/backend/src/shared/config.rs` - создан модуль конфигурации
- ✅ `crates/backend/src/shared/mod.rs` - добавлен модуль config
- ✅ `crates/backend/src/shared/data/db.rs` - обновлена инициализация БД
- ✅ `crates/backend/src/main.rs` - упрощена логика (удален хрупкий код)
- ✅ `config.toml` - создан файл конфигурации в workspace root
- ✅ `config.toml.example` - файл-пример с документацией

## Решенная проблема

**До:**
- Путь к БД вычислялся через `std::env::current_dir()` + `.parent().parent()`
- Создавались разные базы данных в зависимости от директории запуска
- Хрупкий код - ломался при запуске из неожиданных мест

**После:**
- Путь к БД читается из `config.toml` рядом с `backend.exe`
- Всегда используется одна и та же БД (настроенная в конфиге)
- Надежно работает независимо от директории запуска
- Легко настраивается для разных окружений

## Диагностика проблем

Если backend не может найти БД:

1. Проверьте логи запуска - они покажут используемый путь:
   ```
   INFO backend::shared::config: Loading config from: ...
   INFO backend::shared::data::db: Database path: ...
   ```

2. Убедитесь что `config.toml` существует рядом с `backend.exe`:
   ```powershell
   ls target/debug/config.toml      # для dev
   ls target/release/config.toml    # для release
   ```

3. Проверьте путь в `config.toml`:
   ```powershell
   cat target/debug/config.toml
   ```

4. Проверьте что файл БД существует:
   ```powershell
   ls target/db/app.db
   ```

5. Если config.toml отсутствует - будет использован default:
   ```
   INFO backend::shared::config: Using default embedded configuration
   ```

