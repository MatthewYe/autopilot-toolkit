# Distill uses generic stage states

The runner models every configured stage with `pending`, `active`, `waiting`, `blocked`, `needs-reconciliation`, or `completed`, independent of executor identity. Run status is `active`, `blocked`, `completed`, `aborted`, `superseded`, or `purged`; there is no separate paused state because inactivity does not change durable workflow authorization and an unfinished run remains resumable indefinitely.
