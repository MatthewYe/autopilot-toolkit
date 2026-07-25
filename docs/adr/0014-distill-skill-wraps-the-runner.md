# autopilot-distill wraps the distill runner

Users invoke Distill through the `autopilot-distill` agent skill inside an existing conversation. `autopilot-distill` wraps the headless `distill` CLI/state-machine runner, translates its current authorization into agent actions, and submits executor evidence back to it; the runner remains the sole authority for persisted state and stage transitions. This preserves a natural Distill experience now and leaves the same machine-readable runner interface available to future visual clients.
