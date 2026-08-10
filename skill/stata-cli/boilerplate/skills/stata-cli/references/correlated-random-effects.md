# Correlated Random Effects and Mundlak Test (Stata 19)

`xtreg, cre` natively fits correlated random-effects (CRE, Mundlak) models,
and `estat mundlak` runs the Mundlak specification test to choose between
RE and FE/CRE. Both new in Stata 19. Verified on Stata 19.5 MP.

## Syntax

```stata
xtreg depvar [indepvars], cre [sa] [vce(...)]
estat mundlak
```

- `cre` — correlated random-effects estimator (adds panel means of time-varying regressors)
- `sa` — Swamy-Arora estimator of the variance components in the Mundlak regression
- `estat mundlak` works after `xtreg, re`, `xtreg, cre`, and `xtreg, fe`

## Example

```stata
clear
set seed 19
set obs 1000
gen id = ceil(_n/10)
gen t = mod(_n-1,10)+1
xtset id t
gen x = rnormal() + 0.4*id
gen y = 1 + 0.8*x + id + rnormal()

xtreg y x, re
estat mundlak          // H0: covariates uncorrelated with panel effects

xtreg y x, cre
estat mundlak          // Mundlak test (xt_means = 0)
```

## Notes

- The Mundlak test is robust with `vce(robust)`, `vce(cluster var)`,
  bootstrap, and jackknife — unlike the classical Hausman test.
- Rejecting H0 favors FE/CRE over RE.
- `xtreg, cre` output includes `xit_vars` (time-varying regressors) and
  `xt_means` (panel means) coefficient blocks.
