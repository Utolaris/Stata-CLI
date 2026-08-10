# SVAR via Instrumental Variables: ivsvar (Stata 19)

New estimation command in Stata 19. Fits structural VARs (proxy SVARs)
identified with external instruments, an alternative to short-run constraints
in `svar`. Verified on Stata 19.5 MP.

## Syntax

```stata
ivsvar gmm depvarlist (target = ivlist) [, gmm_options]   // GMM estimator
ivsvar mdist depvarlist (targets = ivlist) [, options]    // minimum distance
```

Important: the target variable must NOT appear in `depvarlist`; it is a
separate dependent variable whose shock is proxied by the instruments.

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
gen y3 = rnormal()
replace y1 = 0.5*L.y1 + 0.2*L.y2 + 0.8*z + rnormal() if t>1
replace y2 = 0.3*L.y1 + 0.5*L.y2 + rnormal() if t>1
replace y3 = 0.2*L.y1 + 0.8*z + rnormal() if t>1

ivsvar gmm y1 y2 (y3 = z)
```

## Postestimation

```stata
irf create irfname, set(myirf, replace) step(8)   // structural IRFs
irf graph irfname
```

`ivsvar gmm` allows any number of instruments for one target shock;
`ivsvar mdist` supports several instruments and several target shocks.
