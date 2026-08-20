---
title: "Runbook - Generate Aggregate QC JSON (tools/domain_analisys.py extension)"
date: 2025-12-29
version: 1
---

> **Архивировано 2026-08-19.** Рунбук описывал расширение скрипта
> `domain_analisys.py`, который удалён: он знал 16 агрегатов из 43 и молча
> выдавал по остальным нули. Всё, что здесь предлагалось, с тех пор сделано
> иначе и лучше: QC-сигналы — подсистема `quality/checks/` со страницами
> нарушений, метрики по коду — `codebase_metrics.json` и страница «Метрики
> проекта», дисциплина CSS — `UI_REGISTRY.md`. Оставлено как след замысла.

## Goal

Получить JSON-артефакт качества агрегатов (QC), который потом можно отдавать через backend API и показывать во frontend.

## Inputs / prerequisites

- Репозиторий открыт в корне workspace (где есть `crates/`).
- Python доступен локально.
- Базовый файл метрик уже генерируется как `aggregate_metrics.json` скриптом `tools/domain_analisys.py`.

## Procedure (v1)

1. Запустить `tools/domain_analisys.py` из корня проекта, чтобы получить `aggregate_metrics.json`.
2. Расширить скрипт, добавив сбор QC-сигналов:
   - наличие `metadata.json` у агрегата,
   - сигналы безопасности (секреты + `Debug`),
   - дисциплина UI/CSS (inline styles, `<style>`),
   - анти-паттерны UI data access (`api_base`, raw fetch).
3. Сохранить отдельный артефакт:
   - `aggregate_qc.json` (рекомендуется держать рядом с `aggregate_metrics.json` или в `data/qc/`).
4. Добавить backend endpoint (admin-only), который читает файл и отдаёт JSON.
5. Во frontend добавить страницу/виджет, который отображает QC по агрегатам (можно расширить `d401_metadata_dashboard`).

## Notes / quality gates

- Если `metadata.json` отсутствует — QC должен явно фиксировать `NO_METADATA`.
- Если найдено поле `password|token|api_key|secret` в contracts и `#[derive(Debug)]` на структуре/DTO — флаг `SECRET_DEBUG`.
- Если найден `style="..."` или `<style>` в UI-компонентах — флаг `INLINE_CSS`.


