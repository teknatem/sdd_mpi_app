# Quick Guide: Using List Refactoring Checklist

## Purpose

This guide helps you refactor list views to match the standard established by `a012_wb_sales`, `a015_wb_orders`, and `a013_ym_order`.

## When to Use

Use the **List View Refactoring Checklist** section in SKILL.md when:

1. Creating a new list view from scratch
2. Refactoring an existing list to modern standards
3. Migrating from native HTML table to Thaw Table components
4. Adding server-side pagination to a list
5. Fixing sort indicator not turning green (common bug)

## Quick Start

### 1. Planning Phase

Before starting, determine:

- **Scope**: List-only or full (with details page)?
- **API Format**: Simple `{items, total}` or full paginated `{items, total, page, page_size, total_pages}`?
- **Reference**: Check `a012_wb_sales` for most complete example

### 2. Backend First

Follow **Backend Checklist** in order:

1. Repository layer (list_sql with pagination)
2. Handler layer (paginated response)
3. Routes (unify endpoints)
4. Contracts (add organization_name to DTO)

**Test API**: Use Postman or curl to verify before moving to frontend

```bash
curl "http://localhost:3000/api/a015/wb-orders/list?limit=50&offset=0&sort_by=order_date&sort_desc=true"
```

### 3. Frontend State

Update `state.rs`:

- ⚠️ **Critical**: Use `HashSet<String>` for `selected_ids`, NOT `Vec<String>`
- Add pagination fields: `page`, `page_size`, `total_count`, `total_pages`

### 4. Frontend UI

Work through sections in order:

1. **Imports** - Copy from "Essential Imports" in Quick Reference Card
2. **Constants** - Add TABLE_ID and COLUMN_WIDTHS_KEY
3. **Signals** - Add RwSignals for filters with Effects
4. **Filter Panel** - Single-row layout with DateRangePicker
5. **Table** - Migrate to Thaw components
6. **Sortable Headers** - ⚠️ **Critical**: Use `get_sort_class` for green indicator

### 5. Testing

Check off items in **Testing & Verification Checklist**:

- Compilation passes
- Sort indicator turns **green** for active column
- Pagination works
- Selection works (checkboxes)
- Money cells right-aligned

## Common Issues & Solutions

### Issue 1: Sort Indicator Not Green

**Symptom**: Triangle stays gray when column is sorted

**Fix**:

```rust
// ❌ Wrong (missing CSS class)
<span>{move || get_sort_indicator(...)}</span>

// ✅ Correct (with CSS class)
<span class=move || state.with(|s| get_sort_class(&s.sort_field, "field_name"))>
    {move || get_sort_indicator(...)}
</span>
```

Don't forget:

1. Import `get_sort_class` from `list_utils`
2. Add `style="cursor: pointer;"` to header div
3. Wrap in `<div class="table__sortable-header">`

### Issue 2: "HashSet not found"

**Symptom**: Compilation error about HashSet

**Fix**:

```rust
// Add at top of file
use std::collections::HashSet;

// In state.rs
pub selected_ids: HashSet<String>, // Not Vec<String>!

// In default
selected_ids: HashSet::new(), // Not Vec::new()!
```

### Issue 3: Filters Not Resetting Page

**Symptom**: Changing filter shows wrong results

**Fix**:

```rust
Effect::new(move |_| {
    let val = filter_field.get();
    state.update(|s| {
        s.filter_field = val;
        s.page = 0; // ⚠️ Must reset page!
    });
    load_data();
});
```

### Issue 4: Money Not Aligned

**Symptom**: Numbers not right-aligned

**Fix**:

```rust
// ❌ Wrong
<TableCell>{order.amount}</TableCell>

// ✅ Correct
<TableCellMoney value=order.amount show_currency=false color_by_sign=false />
```

## Checklist Usage Tips

### For New Lists

✅ **Do**: Start from top, work sequentially through Backend → State → UI
✅ **Do**: Copy-paste patterns from Quick Reference Card
✅ **Do**: Test API before starting frontend
✅ **Do**: Commit after each major section (Backend, State, UI)

### For Refactoring Existing Lists

✅ **Do**: Start with Backend to avoid breaking frontend
✅ **Do**: Use `git diff a012_wb_sales` to compare structures
✅ **Do**: Remove old code as you add new (avoid duplicates)
✅ **Do**: Test incrementally (don't refactor everything at once)

❌ **Don't**: Skip state.rs update (will cause type mismatches)
❌ **Don't**: Forget to remove old imports (DateInput, MonthSelector)
❌ **Don't**: Use Vec for selected_ids (must be HashSet)
❌ **Don't**: Forget init_column_resize (columns won't resize)

## Time Estimates

Based on a012/a015/a013 refactorings:

- **Simple list** (no filters): 2-3 hours
- **Standard list** (filters + pagination): 4-6 hours
- **Complex list** (many filters + batch ops): 6-8 hours

Breakdown:

- Backend: 25% of time
- State: 10% of time
- UI migration: 50% of time
- Cleanup & testing: 15% of time

## Reference Implementations

**Primary Reference** (copy patterns from here):

- `crates/frontend/src/domain/a012_wb_sales/ui/list/mod.rs`

**Secondary References** (for specific features):

- `a015_wb_orders` - Server-side pagination, single-row filters
- `a013_ym_order` - Clean Thaw UI, organization enrichment
- `a002_organization` - Minimal list (if need simple example)

## Quick Commands

```bash
# Check compilation
cargo check

# Check specific file
cargo check -p frontend --lib

# Find all lists to refactor
rg "class=\"table__data\"" crates/frontend/src/domain/

# Find lists missing Thaw Table
rg "<table" crates/frontend/src/domain/ -A 1
```

## Validation

After refactoring, verify against **Success Criteria**:

1. ✅ Compilation passes (zero errors)
2. ✅ Sort indicator turns green
3. ✅ All checklist items checked
4. ✅ Matches reference implementation structure
5. ✅ No old code left (no duplicates)

## Getting Help

If stuck:

1. Check **Common Pitfalls & Solutions** section in SKILL.md
2. Compare with reference implementation (`git diff a012_wb_sales`)
3. Search for similar pattern in other refactored lists
4. Check Quick Reference Card for copy-paste templates

## Summary

**Remember the Golden Rules:**

1. 🟢 Always use `HashSet<String>` for selected_ids
2. 🟢 Always import `get_sort_class` for sort indicators
3. 🟢 Always reset `page = 0` when filters change
4. 🟢 Always use `TableCellMoney` for monetary values
5. 🟢 Always remove old code after adding new code

**The Bug that Keeps Coming Back:**

- Sort indicator not turning green = Missing `get_sort_class` CSS class on span!
