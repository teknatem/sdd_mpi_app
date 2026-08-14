---
name: domain-modeling
description: Build and sharpen a project's domain model. Use when the user wants to pin down domain terminology or a ubiquitous language, record an architectural decision, or when another skill needs to maintain the domain model.
---

# Domain Modeling

> Адаптировано под этот проект (пути к глоссарию/ADR, русский язык, экономия сборок).
> Апстрим — `mattpocock/skills`.

Actively build and sharpen the project's domain model as you design. This is the *active*
discipline — challenging terms, inventing edge-case scenarios, and writing the glossary and
decisions down the moment they crystallise. (Merely *reading* `CONTEXT.md` for vocabulary is not
this skill — that's a one-line habit any skill can do. This skill is for when you're changing the
model, not just consuming it.)

**Язык сессии и записей — русский** (проект русскоязычный). Код-идентификаторы не переводятся.

## File structure

Репозиторий — один контекст:

```
/
├── CONTEXT.md                      ← глоссарий домена (единственный, в корне)
├── CLAUDE.md                       ← схема именования a0XX/p9XX/dsXX/…, конвенции
├── ARCHITECTURE.md                 ← каталог объектов (генерируется из кода)
└── memory-bank/
    ├── decisions/                  ← ADR-NNNN-slug.md + README.md с конвенцией
    ├── code-standards/             ← конвенции кодирования (НЕ ADR)
    ├── runbooks/                   ← пошаговые инструкции
    ├── lessons/                    ← уроки
    ├── known-issues/               ← известные ограничения
    └── _archive/                   ← исторический слой
```

`CONTEXT.md` и каталог `memory-bank/decisions/` уже существуют — не создавай их заново
и не заводи `docs/adr/` или `CONTEXT-MAP.md`.

## Перед сессией: прочитай, что уже решено

Проект держит приоритет источников: **код > `CONTEXT.md` > авто-память > `memory-bank/` > `docs/`**.
До первого вопроса прочитай `CONTEXT.md` и таблицу именования в `CLAUDE.md`; при необходимости
`ARCHITECTURE.md` (каталог объектов) и авто-память (бизнес-факты WB/YM/ГК).
Не спрашивай то, что там зафиксировано.

## During the session

### Challenge against the glossary

When the user uses a term that conflicts with the existing language in `CONTEXT.md`, call it out
immediately. «В глоссарии „реализация“ — это официальный отчёт маркетплейса, а ты сейчас,
кажется, про факт продажи. Что имеется в виду?»

### Sharpen fuzzy language

When the user uses vague or overloaded terms, propose a precise canonical term. Особенно осторожно
с местами, где в этом проекте термины уже расходились: `dsXX` («Схемы таблиц») против `dvXX`
(«DataView»), слои `fact` / `fina` / `ybuh`, `a017` (виртуальный сотрудник) против `a038`
(техническое подключение), «показы» против «видимости» у `a040`.

### Discuss concrete scenarios

Stress-test relationships with specific scenarios. Хорошие зацепки в этом домене: заказ, который
отменили после выкупа; реализация, попавшая в другой период; кабинет с FBS и FBY одновременно;
документ, который придётся репостить после смены маппинга.

### Cross-reference with code

When the user states how something works, check whether the code agrees. If you find a
contradiction, surface it.

**Разведку делай через Grep/Read/Glob, а не через сборку.** Компиляция здесь — самая дорогая
операция (`cargo check -p backend` ≈ минуты, wasm-таргет ≈ минута), а во время интервью
ничего не собирается: факты берутся из исходников и документации. Никаких `cargo build`,
`cargo test`, `trunk` в ходе грилла.

### Update CONTEXT.md inline

When a term is resolved, update `CONTEXT.md` right there. Don't batch these up — capture them as
they happen. Формат — [CONTEXT-FORMAT.md](./CONTEXT-FORMAT.md).

`CONTEXT.md` — только глоссарий, без деталей реализации: не спека, не черновик, не свалка решений.

### Offer ADRs sparingly

Only offer to create an ADR when all three are true:

1. **Hard to reverse** — the cost of changing your mind later is meaningful
2. **Surprising without context** — a future reader will wonder "why did they do it this way?"
3. **The result of a real trade-off** — there were genuine alternatives and you picked one for
   specific reasons

If any of the three is missing, skip the ADR. Формат, путь и нумерация —
[ADR-FORMAT.md](./ADR-FORMAT.md). Конвенция кодирования — не ADR: она идёт в
`memory-bank/code-standards/`.
