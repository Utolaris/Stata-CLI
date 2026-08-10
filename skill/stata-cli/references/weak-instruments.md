# Weak-Instrument-Robust Inference: estat weakrobust (Stata 19)

New in Stata 19. After `ivregress`, run `estat weakrobust` for inference that
stays valid with weak instruments. Verified on Stata 19.5 MP.

## Syntax

```stata
ivregress 2sls depvar [indepvars] (endog = instruments)
estat weakrobust [, options]
```

Reported tests:

- Just-identified models: Anderson-Rubin (AR) test
- Overidentified models with a homoskedastic VCE: conditional likelihood-ratio
  (CLR) test by default (also available: AR, Lagrange multiplier)

## Example

```stata
clear
set seed 5
set obs 2000
gen z1 = rnormal()
gen z2 = rnormal()
gen x = z1 + z2 + rnormal()
gen y = 1 + 0.8*x + rnormal()

ivregress 2sls y (x = z1 z2)
estat weakrobust
```

Output: "Test robust to weak instruments", the CLR statistic and p-value, and
a note about which test is reported and why.

## Notes

- Pair with `estat firststage` to see the first-stage F statistic.
- Unlike a plain Wald test after 2SLS, these tests are valid even when the
  instruments are weak.
