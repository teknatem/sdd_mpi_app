You are updating a Memory-Bank stored in Obsidian (Markdown + YAML frontmatter).

Task:
Analyze the full chat session and produce Memory-Bank updates that will reduce future uncertainty and repeated clarifications.

Rules:

- Do NOT invent facts. Only record information that is explicitly established in the chat.
- If something is inferred, label it as "assumption" and keep it out of permanent facts unless confirmed.
- Prefer multiple small reusable notes over one large note.
- Each note must be actionable and searchable.
- Include dates and stable naming.
- Avoid private/sensitive personal data unless strictly necessary for the project.

Output format:
Return ONLY a YAML block called `files:` where each item has:

- path: (relative Obsidian path)
- filename:
- content: (full Markdown with YAML frontmatter)

Create:

1. One Session Debrief note:

   - Summary
   - Main difficulties (what caused uncertainty, what information was missing)
   - Resolutions (what finally clarified it)
   - Links to created notes
   - TODO / open questions

2. Atomic notes as needed, chosen from:
   - Lessons learned
   - Runbook / step-by-step procedure
   - Known issue / pitfall + detection + fix
   - Decision record (ADR) including alternatives and rationale
   - Glossary terms (if new terms appeared)
   - Prompt pattern (if a reusable command/prompt emerged)

Taxonomy:

- Put debriefs into: memory-bank/debriefs/
- Put runbooks into: memory-bank/runbooks/
- Put known-issues into: memory-bank/known-issues/
- Put decisions into: memory-bank/decisions/
- Put glossary into: memory-bank/glossary/
- Put prompt-patterns into: memory-bank/prompt-patterns/
- Put lessons into: memory-bank/lessons/

Naming:

- Debrief: YYYY-MM-DD**session-debrief**<topic>.md
- Runbook: RB**<topic>**v1.md
- Known issue: KI**<topic>**YYYY-MM-DD.md
- Decision: ADR**####**<title>.md
- Glossary: GLO\_\_<term>.md
- Prompt pattern: PP\_\_<pattern>.md
- Lesson: LL**<topic>**YYYY-MM-DD.md

## Execution Steps:

1. **Generate YAML output** with all files content
2. **IMMEDIATELY create actual files** in the filesystem:
   - Create necessary directories if they don't exist
   - Write each file using the Write tool
   - Use absolute paths from workspace root (f:\dev\sdd_mpi_app\)
3. **Verify files are created** by listing directories
4. **Use current year** (2025) in dates, not 2024

Important: Do NOT just output YAML - you MUST physically create the files!
