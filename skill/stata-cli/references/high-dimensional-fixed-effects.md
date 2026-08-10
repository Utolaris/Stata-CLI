# High-Dimensional Fixed Effects (HDFE) — Stata 19

`areg`, `xtreg, fe`, and `ivregress 2sls` can absorb multiple
high-dimensional categorical variables natively. New in Stata 19; verified on
Stata 19.5 MP. Absorbed levels are not estimated, so computation is much
faster than adding indicator variables.

## Syntax

```stata
areg depvar [indepvars], absorb(absvar [absvar ...]) [options]
xtreg depvar [indepvars], fe absorb(absvar [absvar ...])
ivregress 2sls depvar [indepvars] (endog = instruments), absorb(absvar [absvar ...])
```

`absvar` may be a variable or an interaction such as `firm#year`.

## Examples

```stata
* Repeated cross-sections: absorb company, year, and industry together
areg y x, absorb(firm year ind)

* Panel FE with extra absorbed dimensions
xtset firm year
xtreg y x, fe absorb(ind)

* 2SLS with absorbed fixed effects
ivregress 2sls y x (w = z), absorb(firm year)
```

## Notes

- The output reports an "Absorbed variable | Levels" table instead of dummy
  coefficients; the absorbed F test is printed unless suppressed.
- `absorb()` accepts factor-variable interactions: `absorb(firm#year ind)`.
- Combine with `vce(cluster var)` as usual; absorption happens before
  clustering.
- This is the native replacement for the community `reghdfe` in the common
  linear cases above.
