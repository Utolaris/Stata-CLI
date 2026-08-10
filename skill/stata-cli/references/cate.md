# Conditional Average Treatment Effects: cate (Stata 19)

New in Stata 19. Estimates IATEs (individualized) and GATEs (group average
treatment effects), going beyond the overall ATE. Verified on Stata 19.5 MP.

## Syntax

```stata
cate po (ovar catevarlist) (tvar) [, options]        // partialing-out
cate aipw (ovar catevarlist) (tvar) [, options]      // augmented IPW
```

Key options:

- `group(varname)` — estimate GATEs by levels of a factor variable in `catevarlist`
- `group(#)` — data-driven GATE groups
- `controls(varlist)` — high-dimensional controls for outcome and treatment
- `xfolds(#)` — number of cross-fit folds (default 10)

## Examples

```stata
clear
set seed 11
set obs 1000
gen x1 = rnormal()
gen x2 = rnormal()
gen g = floor(runiform()*4)+1
gen treat = rbinomial(1,0.5)
gen y = 1 + 0.5*treat + 0.3*x1 + 0.2*x2 + 0.4*treat*x1 + rnormal()

* IATEs via partialing out (lasso outcome/treatment, random-forest CATE model)
cate po (y x1 x2 i.g) (treat)

* GATEs per group (group variable must be a factor in catevarlist)
cate, reestimate group(g)
```

## Postestimation

```stata
categraph histogram        // distribution of estimated IATEs
categraph gateplot         // GATE estimates with intervals
categraph iateplot         // IATE function vs a CATE variable
predict iates              // predicted IATEs
```

Output shows the estimator, cross-fit folds, ATE, and (with `group()`) the
GATE table.
