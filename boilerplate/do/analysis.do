capture log close
clear all
set more off

// Create an outputs directory for logs, tables, and exported figures.
cap mkdir "outputs"
log using "outputs/result.txt", text replace

// Record basic run metadata so AI agents and users can trace the execution.
display "Run started: $S_DATE $S_TIME"
display "Working directory: `c(pwd)'"

// Load data here.
// use "data/example.dta", clear

// Inspect the dataset before modeling.
describe
summarize

// Main analysis example.
// regress y x1 x2
// estimates store ols

// Export regression tables with esttab when the estout package is available.
// This produces a compact RTF table with coefficients, standard errors,
// sample size, R-squared, adjusted R-squared, and conventional significance stars.
// esttab ols using st_reg.rtf, replace b(%12.3f) se(%12.3f) nogap compress s(N r2 r2_a) star(* 0.1 ** 0.05 *** 0.01)

log close
