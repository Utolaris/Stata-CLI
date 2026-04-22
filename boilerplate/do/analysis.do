capture log close
clear all
set more off

cap mkdir "outputs"
log using "outputs/result.txt", text replace

display "Run started: $S_DATE $S_TIME"
display "Working directory: `c(pwd)'"

* Load data here
* use "data/example.dta", clear

* Inspect the dataset
describe
summarize

* Main analysis
* regress y x1 x2

log close
