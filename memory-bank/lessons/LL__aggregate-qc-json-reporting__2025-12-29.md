---
title: "Lesson - Aggregate QC JSON reporting (metadata + system signals)"
date: 2025-12-29
tags: ["quality-control", "metadata", "frontend", "contracts", "scripts"]
---

## Context

Нужен единый способ понять качество агрегата **без чтения кода**: безопасность, консистентность данных, соблюдение UI/CSS стандартов, признаки переусложнения.

В проекте уже есть:

- Field Metadata System (`metadata.json` → `metadata_gen.rs`) как “паспорт”.
- `tools/domain_analisys.py` как табличный сбор метрик по слоям (bytes). Скрипт удалён 2026-08-19: знал 16 агрегатов из 43. Роль заняли `codebase_metrics.json` и `quality/checks/`.

## Pattern: QC JSON as a build/artifact

Собираем JSON с QC-полями, который можно:

- генерировать периодически (например nightly),
- отдавать через backend API,
- показывать во frontend (как “Quality Dashboard”).

### Recommended QC signals (minimum set)

- **Metadata coverage**
  - `has_metadata_json`
  - `metadata_fields_count`
  - `metadata_has_ai_description`
- **Security**
  - `secrets_in_contracts` (по именам `password|token|api_key|secret`)
  - `debug_derives_on_secret` (секреты + `#[derive(Debug)]` = риск утечки)
- **UI/CSS discipline**
  - `frontend_inline_style_count` (встречаемость `style="..."`)
  - `frontend_style_tag_count` (встречаемость `<style>` в компонентах)
- **UI data access standardization**
  - `frontend_api_base_dup` (наличие `fn api_base()`)
  - `frontend_raw_fetch_hits` (по `window.fetch|RequestInit`)
- **Complexity**
  - `bytes_by_layer` (уже есть в `aggregate_metrics.json`)
  - derived flags for outliers (например “details слишком большой”)

### Output structure

- `aggregate_metrics.json` (existing): bytes split by layer
- `aggregate_qc.json` (new): qc fields + flags + optional score

## Observation

В текущем репозитории `metadata.json` присутствует только у `a001_connection_1c`, значит rollout QC по метаданным можно визуализировать сразу как “coverage gap”.


