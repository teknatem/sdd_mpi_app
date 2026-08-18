---
type: lesson-learned
date: 2026-01-20
topic: Clarify Versioning Requirements Early
tags: [requirements, versioning, planning, communication]
severity: medium
---

# Lesson Learned: Clarify Versioning Requirements Early

## Context
During implementation of `a019_llm_artifact` aggregate for storing LLM-generated SQL queries, the initial requirements included a versioning system with `parent_id` and `version` fields to track artifact evolution.

## What Happened

### Initial Plan
- Created detailed implementation plan including versioning system
- Designed database schema with `parent_id` and `version` fields
- Planned repository functions for version chains
- Designed UI with "Versions" tab for viewing version history

### Requirement Change
Midway through planning, user explicitly stated:
> "Пока не нужно делать систему версий. Просто у чата может быть несколько разных артефактов. У каждого артефакта должен быть комментарий и наименование. Что это новая версия и чем она отличается можно указать в комментарии."

**Translation**: "No need for a versioning system yet. A chat can just have multiple different artifacts. Each artifact should have a comment and name. What it's a new version and how it differs can be noted in the comment."

### Impact
- Required updating the entire plan
- Removed versioning-specific fields and logic
- Simplified implementation significantly
- Avoided implementing unused/unwanted features

## Lesson

### What Went Wrong
**Assumption without confirmation**: Assumed that "сравнивать разные версии" (compare different versions) meant implementing a formal versioning system, rather than just having multiple independent artifacts that users could manually compare.

### Root Cause
**Ambiguous requirements language**: Terms like "версия" (version) can mean:
1. Formal versioning system with parent-child relationships
2. Multiple independent items that users think of as "versions"
3. Simple iteration/variation

### What Worked
- User caught the misunderstanding early (before implementation started)
- Plan was detailed enough that versioning assumptions were visible
- Flexible architecture made it easy to remove versioning without breaking design

## Recommendations

### For AI Agent (Me)
1. **Ask clarifying questions** when versioning is mentioned:
   - "Do you need automatic version tracking with parent-child relationships?"
   - "Or should users manually manage multiple independent artifacts?"
   - "Should the system enforce version lineage or just allow multiple items?"

2. **Distinguish between**:
   - **System-managed versioning**: parent_id, version numbers, version chains
   - **User-managed versions**: Multiple independent records with descriptive names
   - **Audit trail**: Tracking changes over time (different from user-facing versions)

3. **Present trade-offs explicitly**:
   - "Option A: Versioning system (more complex, automatic tracking)"
   - "Option B: Independent artifacts (simpler, manual management)"
   - "Which better fits your use case?"

### For Users/Teams
1. **Be specific about versioning needs**:
   - ❌ "We need versions" (ambiguous)
   - ✅ "We need a version dropdown showing parent-child history" (specific)
   - ✅ "Users should be able to create multiple artifacts and manually note which is newer" (specific)

2. **Consider deferring versioning**:
   - Versioning is complex and often over-engineered initially
   - Start with simple multiple records
   - Add formal versioning later if usage patterns demand it

3. **Review plans before implementation**:
   - Detailed plans surface assumptions
   - Catching issues in planning is 10x cheaper than in code

## Related Patterns

### Similar Requirements Ambiguity
- "History" - audit log vs. user-facing version history?
- "Archive" - soft delete vs. separate archive table?
- "Copy" - full deep copy vs. reference copy?
- "Template" - immutable template vs. starting point?

### Resolution Pattern
When hearing ambiguous terms:
1. **Pause and clarify** before planning
2. **Present concrete examples** of what each interpretation means
3. **Show UI mockups** if needed to make interpretation clear
4. **Confirm understanding** before proceeding

## Status
✅ **Resolved**: User clarified requirements, plan updated, implementation succeeded without versioning system.

## References
- [[2026-01-20_session-debrief_llm-artifact-implementation|Session Debrief]]
- [[RB_add-new-aggregate-sdd_v1|Runbook: Adding New Aggregate]]
