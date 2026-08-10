# Instrumental-Variables Local Projections: ivlpirf (Stata 19)

New in Stata 19. Local-projection impulse-response functions that instrument
an endogenous impulse variable; also see plain `lpirf` for the non-IV version.
Verified on Stata 19.5 MP.

## Syntax

```stata
ivlpirf depvarlist [if] [in] [, endogenous(impulse = ivlist) cumulative step(#) options]
```

Key options:

- `endogenous(impulse = ivlist)` — instrument the impulse variable with one or
  more exogenous instruments (required for IV identification)
- `cumulative` — cumulative IRFs
- `step(#)` — forecast horizon; default 8
- `lags(numlist)` / `exog(varlist)` — controls

## Example

```stata
clear
set seed 17
set obs 1000
gen t = _n
tsset t
gen z = rnormal()
gen y1 = rnormal()
gen y2 = rnormal()
replace y1 = 0.5*L.y1 + 0.2*L.y2 + 0.8*z + rnormal() if t>1
replace y2 = 0.3*L.y1 + 0.5*L.y2 + rnormal() if t>1

ivlpirf y1 y2, endogenous(y1 = z) step(4)
ivlpirf y1 y2, endogenous(y1 = z) cumulative
```

## Postestimation

```stata
estat        // hypothesis tests on single or joint IRFs
irf create / irf graph   // tables and graphs of the estimated IRFs
```

The estimation table reports IRF coefficients at each horizon
(`--.`, `F1.`, `F2.`, ...) with robust standard errors.
