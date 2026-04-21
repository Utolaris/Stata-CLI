capture log close
clear all
set more off

use "grilic.dta", clear
display "scene smoke test"
summarize lnw s expr
