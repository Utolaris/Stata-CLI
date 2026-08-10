# GMM with Panel-Style Instruments: xtinstruments() (Stata 19)

`gmm` with `xtinstruments()` builds instruments whose availability grows with
the time period (dynamic panel / Arellano-Bond style) and is markedly faster
in Stata 19, especially with many panels and numerical derivatives. Verified
on Stata 19.5 MP.

## Syntax

```stata
gmm (residual_equation), xtinstruments([reqlist:]varlist, lags(#1/#2)) [options]
```

Requirements:

- `xtset` your data before using `xtinstruments()`
- With XT-style instruments you MUST specify the initial weight matrix for
  first-differenced equations: `winitial(xt D)` (a string of `L`/`D` letters,
  one per residual equation)
- `lags(#1/#2)` — use lags `#1` through `#2` as instruments; `#2 = .` means
  all available lags

## Example: Arellano-Bond AR(1) on synthetic data

```stata
clear
set seed 23
set obs 600
gen id = ceil(_n/6)
gen t = mod(_n-1,6)+1
xtset id t
gen y = rnormal()
replace y = 0.5*L.y + rnormal() if t>1

gmm (D.y - {rho}*LD.y), xtinstruments(y, lags(2/.)) winitial(xt D) onestep
display _b[/rho]
```

With strictly exogenous regressors, add them to `instruments()`:

```stata
gmm (D.n - {rho}*LD.n - {xb:D.w LD.w D.k LD.k}), ///
    xtinstruments(n, lags(2/.))                   ///
    instruments(D.w LD.w D.k LD.k, noconstant)    ///
    winitial(xt D) onestep
```

## Notes

- The residual equation is written WITHOUT `= 0` in this form
  (`(D.y - {rho}*LD.y)`, not `(D.y - {rho}*LD.y = 0)`).
- `gmm` drops observations with no valid instruments but keeps periods with
  only a subset of the requested lags.
- Output lists "XT-style" instruments separately from standard instruments.
