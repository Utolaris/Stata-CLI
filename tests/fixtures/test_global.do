sysuse auto, clear

global CV "mpg rep78 headroom trunk weight length turn displacement gear_ratio"

reg price foreign $CV