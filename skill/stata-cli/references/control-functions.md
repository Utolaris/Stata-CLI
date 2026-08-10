# Control-Function Models: cfregress / cfprobit (Stata 19)

New in Stata 19. Fit linear and probit models with endogenous regressors using
the control-function approach. Verified on Stata 19.5 MP.

## Syntax

```stata
cfregress depvar [indepvars] (endog1 = ivs1 [, cfopts]) [(endog2 = ivs2, cfopts)] [, options]
cfprobit  depvar [indepvars] (endog1 = ivs1 [, cfopts]) [(endog2 = ivs2, cfopts)] [, options]
```

Endogenous regressors can be continuous, binary, fractional, or count; the
first-stage model is chosen automatically from the variable type.

## Examples

```stata
clear
set seed 13
set obs 1500
gen z = rnormal()
gen u = rnormal()
gen x = 0.8*z + u + rnormal()
gen y = 2 + 0.6*x + u + rnormal()
gen yb = (2 + 0.6*x + u + rnormal() > 0)

cfregress y (x = z)          // linear outcome, continuous endogenous x
cfprobit yb (x = z)          // binary outcome
```

## Notes

- The output reports the main equation plus the "Endogenous variable model"
  and a `cf(x)` control-function term whose significance signals endogeneity.
- `vce(robust)`, `vce(cluster var)`, and HAC standard errors are available.
- Postestimation: `estat`, `predict`, and standard `margins` workflows apply.
