---
title: Thaw UI Migration Runbook
version: 1
date_created: 2025-12-20
last_updated: 2025-12-20
applies_to: [frontend, ui-migration]
tags: [runbook, thaw-ui, migration, procedure]
---

# Runbook: Migrating Pages to Thaw UI

## Purpose

Systematic procedure for migrating Leptos pages from custom HTML/CSS to Thaw UI components while maintaining functionality and theme consistency.

## Prerequisites

- Thaw UI library already added to project dependencies
- Understanding of Leptos signals and components
- Access to CSS variables in `themes/core/variables.css`

## Step-by-Step Procedure

### 1. Analysis Phase

**Read the target file** and identify:

- All form inputs (checkboxes, text inputs, textareas, buttons)
- Layout containers (divs with flexbox/grid)
- Interactive elements (modals, dialogs, pickers)
- Styling patterns (colors, spacing, borders)

**Inventory Thaw replacements**:

- `<input type="checkbox">` → `Checkbox`
- `<input type="text">` → `Input`
- `<textarea>` → `Textarea`
- `<button>` → `Button`
- Layout divs → `Space`, `Flex`, `Grid`
- Custom modals → `Dialog`
- Tables → `Table` components

**Note limitations**:

- No DatePicker (keep HTML `<input type="date">`)
- No simple Select (keep HTML `<select>` or use complex Combobox)

### 2. Import Phase

Add to imports:

```rust
use thaw::*;
```

### 3. Signal Conversion Phase

For each form element that will use Thaw components:

**Before:**

```rust
let (field_name, set_field_name) = signal(default_value);
```

**After:**

```rust
let field_name = RwSignal::new(default_value);
```

**Why**: Thaw form components use direct `RwSignal` binding via their `value=` or `checked=` props.

### 4. Component Replacement Phase

#### Checkboxes

**Before:**

```rust
<label>
    <input
        type="checkbox"
        prop:checked=move || field.get()
        on:change=move |ev| set_field.set(event_target_checked(&ev))
    />
    "Label text"
</label>
```

**After:**

```rust
<Checkbox
    checked=field
    label="Label text"
/>
```

#### Buttons

**Before:**

```rust
<button
    style="padding: 10px 20px; background: #007bff; ..."
    on:click=handler
    prop:disabled=move || condition.get()
>
    "Button text"
</button>
```

**After:**

```rust
<Button
    appearance=ButtonAppearance::Primary
    on_click=handler
    disabled=Signal::derive(move || condition.get())
>
    "Button text"
</Button>
```

#### Input fields

**Before:**

```rust
<input
    type="text"
    value=move || field.get()
    on:input=move |ev| set_field.set(event_target_value(&ev))
/>
```

**After:**

```rust
<Input
    value=field
    placeholder="Hint text"
/>
```

### 5. Layout Update Phase

Replace layout divs with Thaw layout components:

**Vertical stacking:**

```rust
<Space vertical=true>
    // components
</Space>
```

**Horizontal spacing:**

```rust
<Space>
    // components
</Space>
```

**Flex layout:**

```rust
<Flex justify=FlexJustify::SpaceBetween align=FlexAlign::Center>
    // components
</Flex>
```

### 6. Styling Enhancement Phase

Update inline styles to use CSS variables:

**Background colors:**

- `background: #f5f5f5` → `background: var(--color-background-secondary)`
- `background: white` → `background: var(--color-background-primary)`

**Border colors:**

- `border: 1px solid #ddd` → `border: 1px solid var(--color-border)`

**Text colors:**

- `color: #666` → `color: var(--color-text-secondary)`
- Error text → `color: var(--color-error)`

**Brand colors:**

- Use `var(--colorBrandForeground1)` for primary brand elements

**Modern radii:**

- Small elements: `border-radius: 4px`
- Cards/sections: `border-radius: 6px` or `8px`

### 7. Compilation Check

```bash
cargo check -p frontend --target wasm32-unknown-unknown --profile wasm-dev
```

**Common errors:**

- Missing `RwSignal` conversion → convert signal
- Wrong prop names → check Thaw docs
- `Send + Sync` issues → use `Callback` instead of `Rc<dyn Fn>`

### 8. Visual QA

1. Пересобрать фронт: `powershell -File tools/build_frontend.ps1`, открыть http://localhost:3000
2. Navigate to the migrated page
3. Check:
   - All components render correctly
   - Theme switching works (light/dark)
   - Interactions work (clicks, inputs)
   - Modals appear above dialogs (z-index)
   - Layout is responsive

## Success Criteria

- ✅ Code compiles without errors
- ✅ All interactive elements work
- ✅ Visual consistency with other Thaw pages
- ✅ Theme variables used throughout
- ✅ No console errors at runtime

## Rollback Procedure

If migration causes issues:

1. Revert the file: `git checkout HEAD -- <file_path>`
2. Recompile: `cargo check`
3. Document the issue in `memory-bank/known-issues/`

## Related Documents

- [[LL-thaw-html-hybrid-2025-12-20]] - When to keep HTML components
- [[UI_STANDARDS_README]] - UI standards and patterns
- Project brief: Migration to Thaw UI for consistency
