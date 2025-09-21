; This cuts out the plates for mounting the bed to a aluminum extrusion via the linear carriage rails.
;
; Input is in a long bar of 25mm x >50mm bar of aluminum positioning width the longer side along the x axis.
;
;
; Before running this:
; - Anchor 1 is (-360.158, -234.568)
;
; - Probe the Z zero position
;   G38.2 Z-10
; - Set the coordinate position to the bottom left corner of the aluminum
;   G10 L2 P1 X-360.158 Y-214.56800 Z-110.25


; millimeter mode
; workspace coordinate system #1
; absolute mode
G21 G40 G54
G80 G90 G94

; Drilling linear carriage holes

G0 F1000
M6 T6 ; Pick up 1.6mm drill bit

G0 X16 Y6.5 F1000

G0 X16 Y6.5 Z1 F1000
M3 S10000

G1 Z-6 F10
G1 Z1 F10

G0 X16 Y18.5 Z1 F1000
G1 Z-6 F10
G1 Z1 F10

G0 X29 Y18.5 Z1 F1000
G1 Z-6 F10
G1 Z1 F10

G0 X29 Y6.5 Z1 F1000
G1 Z-6 F10
G1 Z1 F10

M5

; With the 3.175mm bit.

G0 F1000
M6 T4 ; Pick 1/8in cutting bit

G0 X4 Y12.5 F1000
G0 Z1 F1000

M3 S12000

G1 Z-6 F40
G1 Z1 F40

G0 X41 Y12.5 Z1 F1000
G1 Z-6 F40
G1 Z1 F40

M5

;; Wait for user to adjust the clamping to allow cutting out the 
M0

G21 G40 G54
G80 G90 G94

; Outer cut

G0 X46.5875 Y-3 F1000

M3 S12000

G1 Z-1 F1000
G1 Y28 F40

G1 Z-2 F1000
G1 Y-3 F40

G1 Z-3 F1000
G1 Y28 F40

G1 Z-4 F1000
G1 Y-3 F40

G1 Z-5 F1000
G1 Y28 F40

G1 Z-6 F100
G1 Y-3 F40

M5

