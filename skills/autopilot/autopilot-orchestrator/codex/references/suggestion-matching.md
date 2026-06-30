# Cross-Issue Suggestion Matching

How the orchestrator matches pending cross-issue suggestions to the current issue's AGENT-BRIEF before dispatching the implementer.

## When this fires

Only when `.scratch/<feature>/suggestions.json` exists and contains entries with `status: "pending"`. If the file doesn't exist or has no pending entries, skip matching entirely — do not pass `CROSS_ISSUE_SUGGESTIONS` to the implementer.

## Infer feature directory

- **Local mode**: extract from issue path (e.g. `.scratch/auth/issues/01-login/` → `.scratch/auth/`)
- **GitHub mode**: generate feature slug from issue title → `.scratch/<feature-slug>/`
- If neither can be inferred → skip matching, do not pass `CROSS_ISSUE_SUGGESTIONS`

## Match algorithm

1. Read `.scratch/<feature>/suggestions.json`. Filter entries where `status: "pending"`.
2. For each pending entry, run **dual matching** (either hit counts as a match):
   - **File-path match**: any string in the entry's `files` array appears as a substring anywhere in the AGENT-BRIEF full text (issue body, AC text, file references) → match
   - **Keyword match**: any string in the entry's `keywords` array appears as a substring in the AGENT-BRIEF full text (**case-insensitive**) → match
3. Unmatched entries stay `pending` — do not pass them.
4. Assemble matched entries into `CROSS_ISSUE_SUGGESTIONS` JSON array.

## CROSS_ISSUE_SUGGESTIONS JSON schema

Each matched entry carries full reviewer context:

```json
{
  "source_issue": "#N or <slug>",
  "round": "<N>",
  "content": "<suggestion body>",
  "files": ["path/to/file1.ts", "..."],
  "keywords": ["keyword1", "..."],
  "reviewer_context": "<full excerpt from original REVIEWER_REPORT for this suggestion, with KEYWORDS/FILES annotation lines>"
}
```

### Reconstructing `reviewer_context`

`suggestions.json` stores structured fields (`content`, `files`, `keywords`) without annotation lines. When assembling `CROSS_ISSUE_SUGGESTIONS`, reconstruct `reviewer_context` from the independent fields into the annotated format the implementer expects:

```
- [ ] <content>
  KEYWORDS: <keywords>
  FILES: <files>
```

## No matches

If no pending suggestion matches the current AGENT-BRIEF → do not pass `CROSS_ISSUE_SUGGESTIONS`. The implementer receives no cross-issue context.
