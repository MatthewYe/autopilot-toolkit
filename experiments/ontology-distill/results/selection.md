| Arm | Clause recall | Clause precision | Context recall | Context precision | Historical context recall | Historical context precision | Exact clauses | Exact contexts | Exact both | Mean full chars | Mean hybrid chars |
|---|---|---|---|---|---|---|---|---|---|---|---|
| none | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 433 | 433 |
| generic | 1.000 | 0.281 | 1.000 | 0.263 | 1.000 | 0.190 | 0.000 | 0.000 | 0.000 | 12365 | 6881 |
| stage | 1.000 | 0.845 | 1.000 | 0.610 | 1.000 | 0.438 | 0.042 | 0.042 | 0.000 | 5423 | 2854 |
| keyword | 0.095 | 0.476 | 0.396 | 0.608 | 0.306 | 0.370 | 0.000 | 0.000 | 0.000 | 1898 | 961 |
| ontology | 1.000 | 1.000 | 1.000 | 0.859 | 1.000 | 0.630 | 1.000 | 0.417 | 0.417 | 3952 | 2265 |

Post-hoc decision exclusions verified in every arm: T6 drops `CTX_ADR52` and `CTX_UPSTREAM`; T7 drops `CTX_DSHDOC`; T8 drops `CTX_ADR45`. The target artifact remains excluded too.
