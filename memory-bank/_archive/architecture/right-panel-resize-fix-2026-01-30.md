---
date: 2026-01-30
type: bugfix
status: superseded-by-v2
tags: [frontend, resize, right-panel, ux]
---

**NOTE**: This approach had issues with fast mouse movement. See `right-panel-resize-fix-v2-2026-01-30.md` for the final solution.

# Right Panel Resize Fix (2026-01-30)

## Проблема

Пользователи не могли нормально изменять размер правой панели:
1. Resizer было почти невозможно захватить (6px слишком узко)
2. При движении мыши панель "отставала" из-за CSS transition
3. Панель могла расширяться за пределы доступного пространства
4. Отсутствовал визуальный feedback при resize

## Решение

### 1. Увеличена ширина resizer (6px → 12px)

**Файл**: `crates/frontend/static/themes/core/variables.css`

```css
--panel-resizer-width: 12px;
```

**Эффект**: В 2 раза проще захватить мышью

### 2. Убран transition с width

**Файл**: `crates/frontend/static/themes/core/layout.css`

```css
.right-panel {
  transition: padding var(--transition-base); /* убрали width */
}
```

**Эффект**: Панель сразу следует за курсором без задержки

### 3. Добавлен визуальный feedback

**Файл**: `crates/frontend/static/themes/core/layout.css`

```css
.right-panel__resizer {
  transition: background var(--transition-fast);
}

.right-panel--resizing .right-panel__resizer {
  background: var(--color-primary);
  opacity: 0.5;
}
```

**Эффект**: 
- При hover - слабая подсветка
- При drag - яркая подсветка primary цветом

### 4. Динамический расчет максимальной ширины

**Файл**: `crates/frontend/src/layout/right/right.rs`

```rust
let window_width = window.inner_width().unwrap().as_f64().unwrap();
let max_available = window_width - 400.0 - 260.0;
let max_width = max_available.min(window_width * 0.5);

let new_width = (start_width.get_untracked() + dx)
    .max(30.0)
    .min(max_width);
```

**Эффект**: Панель не может:
- Сжать центр меньше 400px
- Занять больше половины экрана
- Выйти за пределы окна

### 5. Cursor на всем экране при resize

**Файл**: `crates/frontend/src/layout/right/right.rs`

```rust
// При начале resize
body.style().set_property("cursor", "col-resize");

// При завершении resize
body.style().set_property("cursor", "");
```

**Эффект**: Курсор col-resize виден даже если мышь ушла с resizer

## Результат

| Метрика | До | После |
|---------|----|----|
| Ширина resizer | 6px | 12px |
| Transition delay | 0.3s | 0s |
| Visual feedback | Нет | Есть |
| Max-width | Статичный (50vw) | Динамический |
| Cursor на экране | Нет | Есть |

## Тестирование

**Действия:**
1. Открыть правую панель
2. Навести на левый край панели
3. Потянуть влево/вправо

**Ожидаемое поведение:**
- ✅ Легко попасть на resizer (12px широкий)
- ✅ Панель сразу следует за мышью
- ✅ Resizer подсвечивается при hover и drag
- ✅ Cursor col-resize на всем экране
- ✅ Панель не перекрывает центр
- ✅ Центр не сжимается меньше 400px

## Файлы изменены

1. `crates/frontend/static/themes/core/variables.css` - увеличен panel-resizer-width
2. `crates/frontend/static/themes/core/layout.css` - убран transition, добавлен feedback
3. `crates/frontend/src/layout/right/right.rs` - динамический max-width, cursor на body

## Related Issues

- Original BEM refactor: `right-panel-bem-refactor.md`
- CSS page structure: `css-page-structure.md`
