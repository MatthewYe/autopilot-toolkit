# Distill state is authoritative over events

Each run keeps an authoritative atomic `state.json` plus an append-only `events.jsonl` audit stream and stable JSON CLI responses. Future clients may render current state and consume incremental events through the same protocol, but v1 adds no resident service or full event-sourced replay; damaged audit events degrade reporting while the validated state snapshot remains authoritative.
