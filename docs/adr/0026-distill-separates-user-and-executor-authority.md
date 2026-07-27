# Distill separates user and executor authority

Only explicit user instructions may abort, purge, or take over a Distill run. Stage executors may declare waiting, blocked, completed, reconciliation, or a reasoned supersession according to their contracts, while the runner validates each transition and records successor links. Distill never treats deletion of published PRDs or issues as implicit authority.
