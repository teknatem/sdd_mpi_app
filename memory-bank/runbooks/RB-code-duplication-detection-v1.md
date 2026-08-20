---
title: Runbook - Code Duplication Detection and Refactoring
version: 1
created: 2025-01-13
tags: [runbook, refactoring, code-quality, grep]
difficulty: intermediate
estimated_time: 1-2 hours
---

# Runbook: Detecting and Eliminating Code Duplication

## Purpose

Systematic approach to finding and removing duplicate utility functions in the codebase, consolidating them into the `shared` module.

## Prerequisites

- Access to project source code
- Grep/search tools available
- Understanding of Rust module system
- Ability to run compilation checks

## Step-by-Step Procedure

### Phase 1: Discovery

#### 1.1 Identify Candidate Functions

Look for utility functions that might be duplicated:
- Date/time formatting functions
- API URL construction
- Number formatting
- Error handling patterns
- Data transformation utilities

#### 1.2 Search for Duplicates

Use grep to find all implementations:

```bash
# Search for function definition
rg "fn function_name\(" --type rust

# More specific: find private functions (more likely to be duplicated)
rg "^fn function_name\(" --type rust

# Count occurrences
rg "fn function_name\(" --type rust --count
```

**Example from session:**
```bash
rg "fn api_base\(" --type rust
# Found: 18 matches (1 in shared, 17 duplicates)

rg "fn format_datetime\(" --type rust  
# Found: 6 matches (1 in shared, 5 duplicates)
```

#### 1.3 Analyze Implementations

For each duplicate found:
1. Read the full implementation
2. Note any variations in logic
3. Check if it uses different dependencies (e.g., chrono vs string parsing)
4. Identify the input/output patterns

### Phase 2: Consolidation Strategy

#### 2.1 Check Existing Shared Module

```bash
# List shared module structure
ls crates/frontend/src/shared/

# Check if utility already exists
rg "pub fn function_name" crates/frontend/src/shared/
```

#### 2.2 Decide Approach

**Option A: Function Already Exists in Shared**
- Verify it handles all use cases from duplicates
- Enhance if needed to support variations
- Document any behavior changes

**Option B: Create New Shared Function**
- Choose appropriate shared submodule (api_utils, date_utils, etc.)
- Make function public
- Add comprehensive tests

**Option C: Create Multiple Variants**
- If variations are significant, create named variants
- Example: `format_datetime()` and `format_datetime_space()`

### Phase 3: Implementation

#### 3.1 Enhance Shared Module (if needed)

```rust
// In shared/module_name.rs

/// Clear documentation of what function does
/// Include example if helpful
pub fn utility_function(param: Type) -> ReturnType {
    // Implementation
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_utility_function() {
        // Comprehensive tests
    }
}
```

#### 3.2 Update Shared Module Exports

Ensure function is exported in `shared/mod.rs`:
```rust
pub mod api_utils;  // Contains the utility
```

#### 3.3 Group Files for Refactoring

Organize duplicates by domain/area:
```
Group 1: a001_connection_1c (2 files)
Group 2: a002_organization (2 files)
...
```

#### 3.4 Refactor Each File

For each file with duplicate:

1. **Add import** at top of file:
   ```rust
   use crate::shared::module_name::utility_function;
   ```

2. **Remove local function**:
   - Read sufficient context (10-20 lines before/after)
   - Use exact string matching for removal
   - Verify no trailing artifacts

3. **Verify compilation** after each file or group

### Phase 4: Verification

#### 4.1 Compilation Check

```bash
# Пересобрать фронт (вотчера в цикле нет)
powershell -File tools/build_frontend.ps1
```

#### 4.2 Search for Remaining Duplicates

```bash
# Should only find the shared version
rg "fn utility_function\(" --type rust

# Verify all imports are in place
rg "use crate::shared::.+::utility_function" --type rust
```

#### 4.3 Run Tests

```bash
# If project has tests
cargo test --lib

# Or specific tests
cargo test shared::module_name
```

## Common Pitfalls

### Pitfall 1: Fuzzy String Matching
**Problem**: Replacement doesn't match due to whitespace differences
**Detection**: Function still exists after replacement attempt
**Fix**: Read more context, match exact formatting

### Pitfall 2: Incomplete Removal
**Problem**: Function body removed but declaration remains
**Detection**: Compilation error about duplicate definitions
**Fix**: Ensure entire function removed from `fn` to closing `}`

### Pitfall 3: Missing Import
**Problem**: Removed function but forgot import
**Detection**: Compilation error about undefined function
**Fix**: Add import statement at top of file

### Pitfall 4: Wrong Module Path
**Problem**: Import path incorrect
**Detection**: Compilation error about module not found
**Fix**: Verify module structure, use correct path

## Checklist Template

Use this checklist for each refactoring batch:

- [ ] Searched for all duplicate implementations
- [ ] Analyzed variations in implementations
- [ ] Checked if shared utility exists
- [ ] Enhanced/created shared utility as needed
- [ ] Added tests for shared utility
- [ ] Grouped files for systematic refactoring
- [ ] For each file:
  - [ ] Added import statement
  - [ ] Removed local function completely
  - [ ] Verified no artifacts remain
- [ ] Checked compilation after group
- [ ] Verified no duplicates remain
- [ ] All tests passing
- [ ] Updated documentation if needed

## Example Session Output

```
Session: Code Duplication Refactoring
Date: 2025-01-13

Discovery:
- Found 17 duplicate api_base() functions
- Found 6 duplicate format_datetime() functions  
- Found 1 duplicate format_number_with_separator()

Actions:
- Enhanced shared/date_utils.rs with format_datetime_space()
- Refactored 17 files to use shared/api_utils::api_base
- Refactored 5 files to use shared/date_utils::format_datetime
- Refactored 1 file to use shared/list_utils::format_number

Results:
✓ ~350 lines of duplicate code removed
✓ All files compile successfully
✓ Single source of truth established
```

## Time Estimates

- Discovery phase: 15-20 minutes
- Strategy planning: 10-15 minutes
- Implementation per file: 2-3 minutes
- Verification: 10-15 minutes
- **Total for 20 files**: 1-2 hours

## Related Documents

- [[LL-shared-module-organization-2025-01-13]]
- [[KI-strreplace-fuzzy-matching-2025-01-13]]
- [[2025-01-13-session-debrief-code-duplication-refactoring]]
