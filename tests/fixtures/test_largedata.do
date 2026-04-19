clear 
set obs 500000
gen id = _n
gen x = rnormal()