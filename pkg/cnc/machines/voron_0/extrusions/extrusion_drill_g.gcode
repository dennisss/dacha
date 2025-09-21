; "Extrusion G"
; - Input: 1 x 100mm 1515 extrusions
; - Drill 2 holes 27.5mm from edge on one side of the extrusion

; millimeter mode
; workspace coordinate system #1
; absolute mode
G21 G40 G54
G80 G90 G94

T6 M04 ; Pick 1/8in cutting bit

M496.3 ; Move to Anchor 1

G10 L20 P0 X0 Y0 Z113.5 ; Set current point as workspace origin.

G00 X27.5 Y7.5 F1000
G00 Z15 F1000

M03 S12000
G01 F100 Z-13
G01 F100 Z15
M05

G01 X72.5 F1000

M03 S12000
G01 F100 Z-13
G01 F100 Z15
M05

G00 Z50 F1000
G00 Y200 F1000