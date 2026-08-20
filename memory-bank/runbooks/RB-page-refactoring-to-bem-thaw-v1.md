# Runbook: Page Refactoring to BEM + Thaw UI Standards

**Version:** 1.0  
**Last Updated:** 2026-01-30  
**Status:** Active

## Purpose

Step-by-step guide for refactoring frontend pages to comply with BEM + Thaw UI standards established in `memory-bank/architecture/frontend-page-standards.md`.

## When to Use

- Refactoring legacy pages to new standards
- After audit identifies non-compliant pages
- When adding features to old pages (refactor first)
- During code review when standards violations found

## Prerequisites

- [ ] Read `memory-bank/architecture/frontend-page-standards.md`
- [ ] Identify target page file path
- [ ] Знать команду пересборки фронта: `powershell -File tools/build_frontend.ps1` (вотчера нет, фронт на :3000)
- [ ] Have reference implementation open for comparison

## Pre-Refactoring Phase

### 1. Assessment

- [ ] **Read the current page code completely**
- [ ] **Identify page type:**
  - [ ] Simple list view
  - [ ] Complex list with filters
  - [ ] Tree view
  - [ ] Details/form view
- [ ] **Note custom logic to preserve:**
  - Special event handlers
  - Complex state management
  - API integration patterns
  - Modal behaviors
- [ ] **Check for shared components:**
  - Which components are imported?
  - Any custom utilities being used?

### 2. Create Backup

- [ ] **Git status check:** Verify working directory is clean
- [ ] **Create branch** (optional): `git checkout -b refactor/aXXX-entity-bem`
- [ ] **Note current line count** for comparison

## Refactoring Phase

### Step 1: Fix Root Container

**Check:** Find the root `<div>` in `view!` macro

**Common issues:**
- `class="content"` → change to `class="page"`
- `style="padding: 20px;"` → remove inline style
- Missing class entirely → add `class="page"`

**Action:**
```rust
// Before
<div class="content">
// or
<div style="padding: 20px;">

// After
<div class="page">
```

- [ ] **Applied container fix**

### Step 2: Fix Header Structure

**Check:** Find header section (usually first child after root)

**Common issues:**
- `class="page-header"` → change to `class="page__header"`
- Complex nesting → simplify to left/right pattern
- Wrong child classes → use `page__header-left` and `page__header-right`

**Action:**
```rust
// Before (bad)
<div class="page-header">
    <div class="page-header__content">
        <div class="page-header__icon">{icon("...")}</div>
        <div class="page-header__text">
            <h1 class="page-header__title">{"Title"}</h1>
        </div>
    </div>
    <div class="page-header__actions">
        // buttons
    </div>
</div>

// After (good)
<div class="page__header">
    <div class="page__header-left">
        <h1 class="page__title">{"Title"}</h1>
    </div>
    <div class="page__header-right">
        // buttons
    </div>
</div>
```

- [ ] **Applied header structure fix**
- [ ] **Fixed title class** (`page-header__title` → `page__title`)

### Step 3: Standardize Table Structure

**Decision point:** Which table pattern to use?

**Use Thaw Table if:**
- Page already uses other Thaw components
- Need sorting, filtering, or complex interactions
- Want automatic theme integration

**Use BEM native table if:**
- Simple data display
- Need full styling control
- Tree view or custom layout

#### Option A: Migrate to Thaw Table

```rust
// Before
<table>
    <thead><tr><th>Code</th></tr></thead>
    <tbody><tr><td>Data</td></tr></tbody>
</table>

// After
<Table>
    <TableHeader>
        <TableRow>
            <TableHeaderCell>{"Code"}</TableHeaderCell>
        </TableRow>
    </TableHeader>
    <TableBody>
        <TableRow>
            <TableCell>
                <TableCellLayout>{"Data"}</TableCellLayout>
            </TableCell>
        </TableRow>
    </TableBody>
</Table>
```

**Required imports:**
```rust
use thaw::*;
```

- [ ] **Added Thaw imports**
- [ ] **Converted to Thaw Table**

#### Option B: Apply BEM Classes to Native Table

```rust
// Before
<table>
    <thead><tr><th>Code</th></tr></thead>
    <tbody><tr><td>Data</td></tr></tbody>
</table>

// After
<div class="table">
    <table class="table__data table--striped">
        <thead class="table__head">
            <tr>
                <th class="table__header-cell">{"Code"}</th>
            </tr>
        </thead>
        <tbody>
            <tr class="table__row">
                <td class="table__cell">{"Data"}</td>
            </tr>
        </tbody>
    </table>
</div>
```

- [ ] **Added table container wrapper**
- [ ] **Applied BEM classes to table elements**

### Step 4: Replace Native Elements with Thaw Components

**Buttons:**
```rust
// Before
<button style="...">{"Action"}</button>

// After (Thaw)
<Button appearance=ButtonAppearance::Primary>
    {"Action"}
</Button>

// Or (native with BEM)
<button class="button button--primary">
    {"Action"}
</button>
```

**Inputs:**
```rust
// Before
<input type="text" value=... />

// After
<Input value=... on_input=... />
```

**Layout:**
```rust
// Before
<div style="display: flex; justify-content: space-between;">

// After
<Flex justify=FlexJustify::SpaceBetween>
```

- [ ] **Replaced buttons**
- [ ] **Replaced inputs**
- [ ] **Updated layout components**

### Step 4A: Details Tab Layout Check

For domain/projection pages in `ui/details/`, verify the tab matches the details-page layout standard.

- [ ] **Choose one allowed pattern:**
  - `TwoColumnOverview`
  - `SingleCard`
  - `DataTab`
- [ ] **Choose one card content type for each card:**
  - `EditCard`
  - `ViewCard`
  - `ActionCard`
  - `ExceptionCard` only for special-review cases
- [ ] **Replace raw `Card` with `CardAnimated`**
- [ ] **Add `nav_id` to every `CardAnimated`**
- [ ] **Remove empty placeholder `<div></div>` blocks**
- [ ] **Normalize inner card structure** instead of keeping ad-hoc inline layout blocks
- [ ] **Stop and exclude the page** if it belongs to generic/shared tooling, dashboard/view, LLM workflow, or system/admin details

Reference:
- `memory-bank/architecture/details-page-layout-standard.md`

### Step 5: Add/Fix Filter Panel (if needed)

**Check:** Does page have filters?

If yes, ensure BEM structure:
```rust
<div class="filter-panel">
    <div class="filter-panel-header">
        <div class="filter-panel-header__left">...</div>
        <div class="filter-panel-header__center">...</div>
        <div class="filter-panel-header__right">...</div>
    </div>
    <Show when=expanded>
        <div class="filter-panel-content">
            // Filter controls
        </div>
    </Show>
</div>
```

- [ ] **Added/fixed filter panel structure**
- [ ] **Applied BEM classes**

### Step 6: Verify CSS Classes Exist

- [ ] **Check layout.css** for all classes used
- [ ] **Check components.css** for component classes
- [ ] **Verify no typos** in class names (common: `page-header` vs `page__header`)

### Step 7: Remove Unnecessary Code

- [ ] **Remove unused imports**
- [ ] **Remove old CSS class definitions** (if in component file)
- [ ] **Remove commented-out code** (if any)
- [ ] **Remove unnecessary modifiers** (like `page--wide`)

## Post-Refactoring Phase

### 1. Compilation Check

```bash
# In workspace root
cargo check --package frontend
```

- [ ] **No compilation errors**
- [ ] **No new warnings introduced**

### 2. Linter Check

- [ ] **Run ReadLints tool** on modified file
- [ ] **Fix any linter errors** (unused imports, etc.)
- [ ] **Verify no new issues** introduced

### 3. Browser Testing

**Test checklist:**
- [ ] **Page loads** without errors (check browser console)
- [ ] **Header displays correctly** (title, buttons visible)
- [ ] **Table renders** with proper styling
- [ ] **Width is 100%** of available space (not fixed 720px)
- [ ] **Buttons work** (create, edit, delete)
- [ ] **Filters work** (if present)
- [ ] **Sorting works** (if present)
- [ ] **Modal opens** correctly (if used)
- [ ] **Theme switching** works (light/dark/forest)

### 4. Documentation Update

- [ ] **Update component docstring** if structure changed significantly
- [ ] **Add note to activeContext.md** if this is part of larger refactor effort
- [ ] **Update progress.md** if milestone reached

## Common Issues and Solutions

### Issue 1: Page Not Full Width

**Symptom:** Page content is narrow (720px or similar)

**Cause:** Missing width/height styles on `.page` or `.tabs__item`

**Fix:** Verify CSS:
```css
.page {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.tabs__item {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
}
```

### Issue 2: Header Not Sticky

**Symptom:** Header scrolls with content

**Cause:** Missing `position: sticky` in CSS or wrong parent container

**Fix:** Verify `.page__header` has:
```css
.page__header {
  position: sticky;
  top: 0;
  z-index: var(--z-header);
}
```

### Issue 3: Table Styling Broken

**Symptom:** Table has no borders, wrong colors

**Cause:** Missing BEM classes or wrapper

**Fix:** Ensure structure:
```rust
<div class="table">
    <table class="table__data table--striped">
```

### Issue 4: Thaw Components Not Styled

**Symptom:** Thaw components look wrong or don't match theme

**Cause:** Missing ConfigProvider in app root

**Fix:** Verify `app_shell.rs` has:
```rust
<ConfigProvider theme>
    // App content
</ConfigProvider>
```

## Examples

### Example 1: a008_marketplace_sales

**Before:**
```rust
view! {
    <div class="content">  // ❌ Wrong class
        <div class="page__header">  // ✅ Correct
            ...
        </div>
        <table>  // ❌ No BEM classes
            ...
        </table>
    </div>
}
```

**Issues identified:**
1. Container uses "content" instead of "page"
2. Table missing BEM classes

**After refactoring:**
```rust
view! {
    <div class="page">  // ✅ Fixed
        <div class="page__header">  // ✅ Already good
            ...
        </div>
        <div class="table">  // ✅ Added wrapper
            <table class="table__data table--striped">  // ✅ Added BEM
                <thead class="table__head">
                    <tr>
                        <th class="table__header-cell">...</th>
                    </tr>
                </thead>
            </table>
        </div>
    </div>
}
```

### Example 2: a016_ym_returns

**Before:**
```rust
view! {
    <div class="page page--wide">  // ❌ Unnecessary modifier
        <div class="page-header">  // ❌ Wrong class (single hyphen)
            <div class="page-header__content">  // ❌ Over-nesting
                <div class="page-header__icon">...</div>
                <div class="page-header__text">
                    <h1 class="page-header__title">{"Title"}</h1>  // ❌ Wrong class
                </div>
            </div>
            <div class="page-header__actions">  // ❌ Wrong class
                ...
            </div>
        </div>
    </div>
}
```

**Issues identified:**
1. Unnecessary `page--wide` modifier
2. Header uses `page-header` instead of `page__header`
3. Over-complicated header nesting
4. Wrong title class

**After refactoring:**
```rust
view! {
    <div class="page">  // ✅ Clean, no modifier
        <div class="page__header">  // ✅ BEM correct
            <div class="page__header-left">  // ✅ Simplified
                <h1 class="page__title">{"Title"}</h1>  // ✅ Correct class
            </div>
            <div class="page__header-right">  // ✅ BEM correct
                ...
            </div>
        </div>
    </div>
}
```

## Verification Checklist

Before marking refactoring complete:

- [ ] All modifications follow BEM naming
- [ ] No inline styles (except unavoidable edge cases)
- [ ] Thaw components used where appropriate
- [ ] CSS classes all exist in stylesheets
- [ ] Cargo check passes
- [ ] ReadLints shows no new errors
- [ ] Browser test confirms layout works
- [ ] Width is 100% (not fixed)
- [ ] Theme switching works correctly

## Time Estimates

- **Simple fixes** (container + header only): 5 minutes
- **Moderate** (container + header + table): 15 minutes
- **Complex** (full refactor with filters + Thaw migration): 30-45 minutes

## Related Documentation

- **Standards:** `memory-bank/architecture/frontend-page-standards.md`
- **Templates:** `memory-bank/templates/page-templates.md`
- **Automated audit:** `.cursor/skills/audit-page-bem-thaw/SKILL.md`

## Rollout Strategy

### Phase 1: High-Traffic Pages (Priority)
1. a002_organization ✅ (already compliant)
2. a012_wb_sales ✅ (recently fixed)
3. a006_connection_mp ✅ (already compliant)

### Phase 2: Pages Needing Fixes
1. a016_ym_returns - header classes
2. a007_marketplace_product - header classes
3. a004_nomenclature/list - header classes
4. a008_marketplace_sales - container + table
5. a005_marketplace - container class
6. a003_counterparty/tree - container + table
7. a004_nomenclature/tree - container + table

### Phase 3: Remaining Pages
- Review all other domain pages
- Apply standards as encountered

## Success Metrics

- **Code consistency:** All pages use same structure
- **Maintainability:** Global style changes easier
- **Developer velocity:** Clear patterns to follow
- **Visual consistency:** Uniform appearance across app
