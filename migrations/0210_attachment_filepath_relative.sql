-- Вложения чатов: путь в БД становится относительным корню вложений.
--
-- Было: `filepath` хранил путь относительно РАБОЧЕГО КАТАЛОГА процесса
-- (`uploads/chat_attachments/<chat_id>/<uuid>.<ext>`, на Windows — с обратными
-- слешами). Это единственное файловое хранилище приложения, чей путь не
-- настраивался конфигом, поэтому запуск бэкенда из другой директории «терял»
-- все вложения, а перенос каталога данных ломал ссылки в БД.
--
-- Стало: хранится ключ относительно корня вложений — `<chat_id>/<uuid>.<ext>`.
-- Абсолютный путь собирается в рантайме от `[llm].attachments_path`
-- (по умолчанию `<data_root>/attachments`).
--
-- Совместимость: код умеет читать обе формы (см. `attachment_abs_path` в
-- domain/a018_llm_chat/service.rs), поэтому порядок «сначала миграция, потом
-- перенос файлов» и обратный равнозначны, а строки, не подошедшие под шаблон,
-- продолжают работать по-старому.

-- Windows-форма с обратными слешами.
UPDATE a018_llm_chat_attachment
   SET filepath = replace(
           substr(filepath, length('uploads\chat_attachments\') + 1),
           '\', '/'
       )
 WHERE filepath LIKE 'uploads\chat_attachments\%';

-- POSIX-форма.
UPDATE a018_llm_chat_attachment
   SET filepath = substr(filepath, length('uploads/chat_attachments/') + 1)
 WHERE filepath LIKE 'uploads/chat_attachments/%';
