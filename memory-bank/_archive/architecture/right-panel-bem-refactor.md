---
date: 2026-01-30
type: refactor
status: completed
tags: [frontend, css, bem, right-panel, resize]
---

# Right Panel BEM Refactor & Resize Fix

## Проблема

1. **Именование не по BEM**: класс `.right` не соответствует методологии BEM
2. **Конфликт имен**: `.right-panel` использовался для контента, но нужен для основного блока
3. **Проблема resize**: правая панель могла перекрывать центральную часть при изменении размера
4. **Отсутствие защиты**: нет ограничений на минимальный размер центральной области

## Решение

### 1. Переименование классов по BEM

| Старый класс      | Новый класс                    | Роль                             |
|-------------------|--------------------------------|----------------------------------|
| `.right`          | `.right-panel`                 | Основной блок (BEM Block)        |
| `.right.hidden`   | `.right-panel--hidden`         | Модификатор скрытия              |
| `.right.resizing` | `.right-panel--resizing`       | Модификатор при resize           |
| `.right-resizer`  | `.right-panel__resizer`        | Element: handle для resize       |
| `.resize-overlay` | `.right-panel__resize-overlay` | Element: оверлей при resize      |
| `.right-panel`    | `.right-panel__content`        | Element: контент панели          |

### 2. Исправления CSS

**Добавлено в `.right-panel`:**
```css
max-width: 50vw;  /* Ограничение максимальной ширины */
flex-grow: 0;     /* Не растягивается */
```

**Добавлено в `.app-main`:**
```css
min-width: 400px; /* Минимальная ширина центра */
```

**Улучшено в `.right-panel__resizer`:**
```css
z-index: 1;       /* Над контентом панели */
```

### 3. Обновленные компоненты

#### right.rs
```rust
class="right-panel"
class:right-panel--hidden=move || !is_open()
class:right-panel--resizing=move || is_resizing.get()
```

#### right_panel.rs
```rust
<div class="right-panel__content">
```

## Механизм Resize

### Как работает

1. **Resizer handle** (`.right-panel__resizer`) - область слева от панели шириной `12px` (увеличено с 6px)
2. **Mouse down** на resizer активирует режим resize
3. **Drag влево** увеличивает ширину панели: `dx = start_x - current_x`
4. **Ограничения**:
   - Минимум: 30px (из JS)
   - Максимум: динамический расчет `min(window_width - 660px, window_width * 0.5)`
   - Центр: минимум 400px (защита от перекрытия)

### Улучшения (2026-01-30)

1. **Увеличена ширина resizer**: 6px → 12px для удобства захвата
2. **Убран transition на width**: устранена задержка при resize
3. **Визуальный feedback**: 
   - При hover - подсветка resizer
   - При drag - яркая подсветка + cursor на всем экране
4. **Динамический max-width**: учитывается реальный размер окна
5. **Cursor на body**: col-resize на всем экране при drag

### Координаты

```
[app-main (flex: 1, min-width: 400px)] | [right-panel (width: Npx, динамический max)]
                                        ^
                                   resizer handle (12px)
                                   
← drag left = увеличить панель
→ drag right = уменьшить панель
```

## Преимущества

1. **BEM соответствие**: все классы следуют методологии
2. **Нет перекрытия**: центр защищен минимальной шириной
3. **Ограничение панели**: не может занять больше половины экрана
4. **Предсказуемость**: четкие границы изменения размера

## Файлы изменены

- `crates/frontend/static/themes/core/layout.css`
- `crates/frontend/src/layout/right/right.rs`
- `crates/frontend/src/layout/right/panel/right_panel.rs`
- `memory-bank/architecture/css-page-structure.md`

## Тестирование

1. Открыть правую панель
2. Потянуть за левый край (resizer)
3. Проверить что панель:
   - Не перекрывает центр (min 400px)
   - Не превышает 50vw
   - Плавно ресайзится
4. Закрыть панель - ширина становится 0

## Related

- BEM Methodology: https://en.bem.info/methodology/
- Flexbox layout для правильного распределения пространства
