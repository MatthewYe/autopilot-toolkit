# Distill skill wraps the runner

Users invoke Distill as an agent skill inside an existing conversation. The skill wraps the headless CLI/state-machine runner, translates its current authorization into agent actions, and submits executor evidence back to it; the runner remains the sole authority for persisted state and stage transitions. This preserves a natural `/distill` experience now and leaves the same machine-readable runner interface available to future visual clients.
