-- Журнал шагов экземпляра: какой Этап отработал, с каким исходом и что записал.
--
-- Состояние экземпляра отвечает на вопрос «где мы сейчас», и этого мало:
-- разбирать придётся прогон, который уже кончился. Без журнала шагов у человека
-- остаётся один `last_outcome` — последний выход последнего Этапа, — а вопрос
-- «как мы сюда попали» не имеет ответа вовсе.
--
-- Почему отдельно от журнала эффектов: там записаны изменения мира, здесь —
-- решения. У Этапа без эффектов (а таких большинство: он читает и ветвится)
-- в журнале эффектов не появляется ничего, но его выход определил маршрут.

CREATE TABLE IF NOT EXISTS sys_process_step (
    id           TEXT PRIMARY KEY,
    instance_ref TEXT NOT NULL,
    stage_code   TEXT NOT NULL,
    -- Номер захода в Этап: в графе бывают циклы, и два шага по одному Этапу
    -- различаются именно им.
    visit        INTEGER NOT NULL DEFAULT 0,
    -- Класс исхода: 'outcome' | 'temporary_failure' | 'defect' (ADR-0011 п.10).
    verdict      TEXT NOT NULL,
    -- Имя выхода графа — только у штатного исхода.
    outcome      TEXT,
    -- Данные выхода либо текст сбоя.
    data_json    TEXT,
    message      TEXT,
    -- Что Этап написал в лог: строки mjs, как есть.
    logs_json    TEXT NOT NULL DEFAULT '[]',
    -- Идентификаторы записей журнала эффектов, созданных этим шагом.
    effects_json TEXT NOT NULL DEFAULT '[]',
    duration_ms  INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sys_process_step_instance
    ON sys_process_step(instance_ref, created_at);
