# Panel-Data VAR: xtvar (Stata 19)

Official panel-data vector autoregression, new in Stata 19. Replaces reliance
on the community `pvar`/`pvar2` commands. Verified on Stata 19.5 MP.

## Syntax

```stata
xtvar depvarlist [if] [in] [, options]
```

Common options:

- `lags(#)` — number of lags of the dependent variables; default `lags(1)`
- `fodeviation` — remove fixed effects with forward-orthogonal deviations (default)
- `fd` — remove fixed effects with first differences
- `endogenous(varlist)` — additional endogenous covariates
- `exogenous(varlist)` — additional exogenous covariates
- `maxldep(#)` — maximum lags of dependent/endogenous/predetermined variables used as instruments
- `collapse` — collapse moment conditions within each panel

## Requirements

You must `xtset` your data first. `xtvar` is a GMM estimator, so keep the
instrument count reasonable: with few panels or long time dimensions, the
moment covariance matrix can be singular. Add `maxldep(1)` or `collapse` to
shrink the instrument set.

## Examples

Public dataset (needs network):

```stata
webuse swedishgov
xtvar expenditures grants revenues
xtvar expenditures grants revenues, lags(3) maxldep(2)
```

Synthetic panel (no network):

```stata
clear
set seed 42
set obs 1200
gen id = ceil(_n/12)
gen t = mod(_n-1,12)+1
xtset id t
gen y1 = rnormal()
gen y2 = rnormal()
replace y1 = 0.5*L.y1 + 0.2*L.y2 + rnormal() if t>1
replace y2 = 0.1*L.y1 + 0.5*L.y2 + rnormal() if t>1
xtvar y1 y2, lags(1) maxldep(1)
```

## Postestimation

```stata
xtvarsoc y1 y2, maxlags(3)      // lag-order selection
xtvarirf create irf1, step(8)   // impulse-response functions
```

Output reports per-equation lag coefficients, Hansen's J statistic, and the
GMM-type instrument list.
