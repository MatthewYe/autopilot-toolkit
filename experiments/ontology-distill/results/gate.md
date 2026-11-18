# Gate results

Fixtures: **23/23** as expected.

## Fixtures

| fixture | tier | expected | actual | messages | ok |
|---|---|---|---|---|---|
| `graph-empty` | blocking | reject | reject | 1 | yes |
| `issues-cycle` | blocking | reject | reject | 1 | yes |
| `issues-dangling` | blocking | reject | reject | 1 | yes |
| `issues-empty-blocked-by` | blocking | reject | reject | 1 | yes |
| `issues-missing-parent` | blocking | reject | reject | 1 | yes |
| `issues-no-ac` | blocking | reject | reject | 1 | yes |
| `issues-no-blocked-by` | blocking | reject | reject | 1 | yes |
| `issues-valid` | blocking | pass | pass | 0 | yes |
| `prd-advisory-ci` | advisory | pass | 1 warn | 1 | yes |
| `prd-advisory-clean` | advisory | pass | pass | 0 | yes |
| `prd-advisory-files` | advisory | pass | 1 warn | 1 | yes |
| `prd-advisory-migration` | advisory | pass | 1 warn | 1 | yes |
| `prd-advisory-runtime` | advisory | pass | 1 warn | 1 | yes |
| `prd-advisory-stories` | advisory | pass | 1 warn | 1 | yes |
| `prd-advisory-validation` | advisory | pass | 1 warn | 1 | yes |
| `prd-empty-problem` | blocking | reject | reject | 1 | yes |
| `prd-empty-solution` | blocking | reject | reject | 1 | yes |
| `prd-missing-problem` | blocking | reject | reject | 1 | yes |
| `prd-no-out-of-scope` | blocking | reject | reject | 1 | yes |
| `prd-no-seam` | blocking | reject | reject | 1 | yes |
| `prd-short-problem` | blocking | pass | pass | 0 | yes |
| `prd-stories-unnumbered` | blocking | reject | reject | 1 | yes |
| `prd-valid` | blocking | pass | pass | 0 | yes |

## Corpus replay

Every row is an artifact a human already approved at its checkpoint.

| artifact | blocking | advisories | advisory messages |
|---|---|---|---|
| `prd-0002` | pass | 2 | P10; P04 |
| `prd-0003` | pass | 2 | P10; P04 |
| `prd-0004` | pass | 3 | P12; P04; P03 |
| `prd-121` | pass | 2 | P12; P04 |
| `issues-121` | pass | 0 | — |

Blocking failures on approved artifacts: **0**.
