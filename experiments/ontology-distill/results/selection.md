| Arm | Clause recall | Clause precision | Context recall | Context precision | Historical context recall | Historical context precision | Exact clauses | Exact contexts | Exact both | Mean full chars | Mean hybrid chars |
|---|---|---|---|---|---|---|---|---|---|---|---|
| none | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 433 | 433 |
| generic | 1.000 | 0.281 | 1.000 | 0.265 | 1.000 | 0.189 | 0.000 | 0.000 | 0.000 | 12401 | 6888 |
| stage | 1.000 | 0.845 | 1.000 | 0.612 | 1.000 | 0.434 | 0.042 | 0.042 | 0.000 | 5447 | 2860 |
| keyword | 0.095 | 0.476 | 0.402 | 0.615 | 0.306 | 0.363 | 0.000 | 0.000 | 0.000 | 1908 | 961 |
| ontology | 1.000 | 1.000 | 1.000 | 0.861 | 1.000 | 0.625 | 1.000 | 0.417 | 0.417 | 3976 | 2270 |

Post-hoc decision exclusions verified in every arm: T6 drops `CTX_ADR52` and `CTX_UPSTREAM`; T8 drops `CTX_ADR45`. The target artifact remains excluded too.
