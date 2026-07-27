# Distill tracks domain-doc changes without owning Git

Domain glossary and ADR edits made by `grill-with-docs` are formal clarification-stage artifacts whose paths and hashes appear in completion evidence. The runner validates and reports them but never stages, commits, reverts, or silently removes working-tree changes; aborted and superseded runs retain an explicit record of any domain-document edits they leave behind.
