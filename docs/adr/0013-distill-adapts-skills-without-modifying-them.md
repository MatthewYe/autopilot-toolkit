# Distill adapts skills without modifying them

Distill-owned stage executor adapters wrap existing natural-language skills with stable input, completion-evidence, and artifact-validation contracts. The vendored `grill-with-docs`, `to-prd`, and `to-issues` skills remain unchanged and independently upgradable; replacing a workflow step changes its adapter assignment rather than coupling the runner to a particular skill's prose or output habits.
