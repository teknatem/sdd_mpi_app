-- Доступ к навыку разбора метрик проекта (app-health-review).
--
-- Без строк здесь навык виден только coordinator_admin: `default_access` в
-- skill_policy.rs отдаёт всем остальным специализациям 'denied', а `default_for`
-- во фронтматтере skill-файла — документация, а не политика.
--
-- Состояние приложения — тема системного администратора; аналитику и
-- разработчику плагинов навык доступен по запросу (extended), потому что
-- «что ухудшилось в кодовой базе» касается их работы, но не является ею.

INSERT OR REPLACE INTO sys_llm_skill_access (specialization, skill_id, access_level)
VALUES
    ('system_admin',      'app-health-review', 'immediate'),
    ('coordinator_admin', 'app-health-review', 'immediate'),
    ('business_analyst',  'app-health-review', 'extended'),
    ('plugin_admin',      'app-health-review', 'extended');
