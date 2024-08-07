EESchema Schematic File Version 4
EELAYER 30 0
EELAYER END
$Descr A4 11693 8268
encoding utf-8
Sheet 3 4
Title ""
Date ""
Rev ""
Comp ""
Comment1 ""
Comment2 ""
Comment3 ""
Comment4 ""
$EndDescr
$Comp
L Device:Polyfuse_Small F?
U 1 1 6461F4A2
P 5450 4200
AR Path="/6461F4A2" Ref="F?"  Part="1" 
AR Path="/645F0C2C/6461F4A2" Ref="F1"  Part="1" 
F 0 "F1" V 5245 4200 50  0000 C CNN
F 1 "100mA" V 5336 4200 50  0000 C CNN
F 2 "Fuse:Fuse_0603_1608Metric" H 5500 4000 50  0001 L CNN
F 3 "~" H 5450 4200 50  0001 C CNN
F 4 "C426238" H 5450 4200 50  0001 C CNN "LCSC"
	1    5450 4200
	0    -1   -1   0   
$EndComp
$Comp
L power:+12V #PWR?
U 1 1 6461F4AE
P 1850 1850
AR Path="/6461F4AE" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F4AE" Ref="#PWR0131"  Part="1" 
F 0 "#PWR0131" H 1850 1700 50  0001 C CNN
F 1 "+12V" V 1865 1978 50  0000 L CNN
F 2 "" H 1850 1850 50  0001 C CNN
F 3 "" H 1850 1850 50  0001 C CNN
	1    1850 1850
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 6461F4B4
P 1500 2050
AR Path="/6461F4B4" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F4B4" Ref="#PWR0132"  Part="1" 
F 0 "#PWR0132" H 1500 1800 50  0001 C CNN
F 1 "GND" V 1505 1922 50  0000 R CNN
F 2 "" H 1500 2050 50  0001 C CNN
F 3 "" H 1500 2050 50  0001 C CNN
	1    1500 2050
	0    -1   -1   0   
$EndComp
$Comp
L Connector:USB_C_Receptacle_USB2.0 J?
U 1 1 6461F4BF
P 1550 4350
AR Path="/6461F4BF" Ref="J?"  Part="1" 
AR Path="/645F0C2C/6461F4BF" Ref="J8"  Part="1" 
F 0 "J8" H 1657 5217 50  0000 C CNN
F 1 "USB_C_Receptacle_USB2.0" H 1657 5126 50  0000 C CNN
F 2 "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12" H 1700 4350 50  0001 C CNN
F 3 "https://www.usb.org/sites/default/files/documents/usb_type-c.zip" H 1700 4350 50  0001 C CNN
	1    1550 4350
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 6461F4C5
P 1550 5450
AR Path="/6461F4C5" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F4C5" Ref="#PWR0133"  Part="1" 
F 0 "#PWR0133" H 1550 5200 50  0001 C CNN
F 1 "GND" V 1555 5322 50  0000 R CNN
F 2 "" H 1550 5450 50  0001 C CNN
F 3 "" H 1550 5450 50  0001 C CNN
	1    1550 5450
	1    0    0    -1  
$EndComp
Wire Wire Line
	1550 5250 1550 5450
Text GLabel 2600 4250 2    50   Input ~ 0
USB_D-
Wire Wire Line
	2350 4250 2250 4250
Wire Wire Line
	2250 4250 2250 4350
Wire Wire Line
	2250 4350 2150 4350
Connection ~ 2250 4250
Wire Wire Line
	2250 4250 2150 4250
Text GLabel 2600 4450 2    50   Input ~ 0
USB_D+
Wire Wire Line
	2350 4450 2250 4450
Wire Wire Line
	2250 4450 2250 4550
Wire Wire Line
	2250 4550 2150 4550
Connection ~ 2250 4450
Wire Wire Line
	2250 4450 2150 4450
$Comp
L power:GND #PWR?
U 1 1 6461F4D8
P 3600 4650
AR Path="/6461F4D8" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F4D8" Ref="#PWR0136"  Part="1" 
F 0 "#PWR0136" H 3600 4400 50  0001 C CNN
F 1 "GND" V 3605 4522 50  0000 R CNN
F 2 "" H 3600 4650 50  0001 C CNN
F 3 "" H 3600 4650 50  0001 C CNN
	1    3600 4650
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 6461F4DF
P 3600 4300
AR Path="/6461F4DF" Ref="R?"  Part="1" 
AR Path="/645F0C2C/6461F4DF" Ref="R34"  Part="1" 
F 0 "R34" H 3659 4346 50  0000 L CNN
F 1 "5.1K" H 3659 4255 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3600 4300 50  0001 C CNN
F 3 "~" H 3600 4300 50  0001 C CNN
F 4 "C21190" H 3600 4300 50  0001 C CNN "LCSC"
	1    3600 4300
	-1   0    0    1   
$EndComp
$Comp
L Device:R_Small R?
U 1 1 6461F4E6
P 3250 4300
AR Path="/6461F4E6" Ref="R?"  Part="1" 
AR Path="/645F0C2C/6461F4E6" Ref="R33"  Part="1" 
F 0 "R33" H 3309 4346 50  0000 L CNN
F 1 "5.1K" H 3309 4255 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3250 4300 50  0001 C CNN
F 3 "~" H 3250 4300 50  0001 C CNN
F 4 "C21190" H 3250 4300 50  0001 C CNN "LCSC"
	1    3250 4300
	-1   0    0    1   
$EndComp
Wire Wire Line
	3250 4050 3250 4200
Wire Wire Line
	3600 3950 3600 4200
Wire Wire Line
	3600 4650 3600 4500
Wire Wire Line
	3600 4500 3250 4500
Wire Wire Line
	3250 4500 3250 4400
Connection ~ 3600 4500
Wire Wire Line
	3600 4500 3600 4400
$Comp
L Diode:MBR0580 D?
U 1 1 6461F4F3
P 2600 3750
AR Path="/6461F4F3" Ref="D?"  Part="1" 
AR Path="/645F0C2C/6461F4F3" Ref="D2"  Part="1" 
F 0 "D2" H 2600 3534 50  0000 C CNN
F 1 "MBR120" H 2600 3625 50  0000 C CNN
F 2 "Diode_SMD:D_SOD-123" H 2600 3575 50  0001 C CNN
F 3 "http://www.mccsemi.com/up_pdf/MBR0520~MBR0580(SOD123).pdf" H 2600 3750 50  0001 C CNN
	1    2600 3750
	-1   0    0    1   
$EndComp
$Comp
L Diode:MBR0580 D?
U 1 1 6461F4F9
P 1800 2400
AR Path="/6461F4F9" Ref="D?"  Part="1" 
AR Path="/645F0C2C/6461F4F9" Ref="D1"  Part="1" 
F 0 "D1" H 1800 2184 50  0000 C CNN
F 1 "MBR120" H 1800 2275 50  0000 C CNN
F 2 "Diode_SMD:D_SOD-123" H 1800 2225 50  0001 C CNN
F 3 "http://www.mccsemi.com/up_pdf/MBR0520~MBR0580(SOD123).pdf" H 1800 2400 50  0001 C CNN
	1    1800 2400
	-1   0    0    1   
$EndComp
Wire Wire Line
	1550 2400 1650 2400
Wire Wire Line
	2200 1950 2200 2400
Wire Wire Line
	2200 2400 1950 2400
Wire Wire Line
	1850 1950 1850 1850
Wire Wire Line
	1400 1950 1850 1950
$Comp
L Device:R_Small R?
U 1 1 6461F507
P 2450 4450
AR Path="/6461F507" Ref="R?"  Part="1" 
AR Path="/645F0C2C/6461F507" Ref="R35"  Part="1" 
F 0 "R35" H 2509 4496 50  0000 L CNN
F 1 "27" H 2509 4405 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 2450 4450 50  0001 C CNN
F 3 "~" H 2450 4450 50  0001 C CNN
F 4 "C23345" H 2450 4450 50  0001 C CNN "LCSC"
	1    2450 4450
	0    1    1    0   
$EndComp
Wire Wire Line
	2600 4450 2550 4450
Wire Wire Line
	2150 4050 3250 4050
Wire Wire Line
	2150 3950 3600 3950
$Comp
L Device:R_Small R?
U 1 1 6461F511
P 2450 4250
AR Path="/6461F511" Ref="R?"  Part="1" 
AR Path="/645F0C2C/6461F511" Ref="R32"  Part="1" 
F 0 "R32" H 2509 4296 50  0000 L CNN
F 1 "27" H 2509 4205 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 2450 4250 50  0001 C CNN
F 3 "~" H 2450 4250 50  0001 C CNN
F 4 "C23345" H 2450 4250 50  0001 C CNN "LCSC"
	1    2450 4250
	0    1    1    0   
$EndComp
Wire Wire Line
	2550 4250 2600 4250
$Comp
L power:+12V #PWR?
U 1 1 6461F518
P 4800 1500
AR Path="/6461F518" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F518" Ref="#PWR0137"  Part="1" 
F 0 "#PWR0137" H 4800 1350 50  0001 C CNN
F 1 "+12V" V 4815 1628 50  0000 L CNN
F 2 "" H 4800 1500 50  0001 C CNN
F 3 "" H 4800 1500 50  0001 C CNN
	1    4800 1500
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 6461F51F
P 4800 1750
AR Path="/6461F51F" Ref="R?"  Part="1" 
AR Path="/645F0C2C/6461F51F" Ref="R28"  Part="1" 
F 0 "R28" H 4859 1796 50  0000 L CNN
F 1 "10K" H 4859 1705 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 4800 1750 50  0001 C CNN
F 3 "~" H 4800 1750 50  0001 C CNN
F 4 "C25804" H 4800 1750 50  0001 C CNN "LCSC"
	1    4800 1750
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 6461F526
P 4800 2150
AR Path="/6461F526" Ref="R?"  Part="1" 
AR Path="/645F0C2C/6461F526" Ref="R31"  Part="1" 
F 0 "R31" H 4859 2196 50  0000 L CNN
F 1 "3.3K" H 4859 2105 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 4800 2150 50  0001 C CNN
F 3 "~" H 4800 2150 50  0001 C CNN
F 4 "C25804" H 4800 2150 50  0001 C CNN "LCSC"
	1    4800 2150
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 6461F52C
P 4800 2400
AR Path="/6461F52C" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F52C" Ref="#PWR0138"  Part="1" 
F 0 "#PWR0138" H 4800 2150 50  0001 C CNN
F 1 "GND" V 4805 2272 50  0000 R CNN
F 2 "" H 4800 2400 50  0001 C CNN
F 3 "" H 4800 2400 50  0001 C CNN
	1    4800 2400
	1    0    0    -1  
$EndComp
Text GLabel 5100 1950 2    50   Input ~ 0
PC_POWER_SENSE
Wire Wire Line
	5100 1950 4800 1950
Wire Wire Line
	4800 1950 4800 2050
Wire Wire Line
	4800 1950 4800 1850
Connection ~ 4800 1950
Wire Wire Line
	4800 2400 4800 2250
Wire Wire Line
	4800 1500 4800 1650
Text Notes 5100 1850 0    50   ~ 0
3V when PC is on.
Wire Wire Line
	2150 3750 2450 3750
$Comp
L power:+5V #PWR?
U 1 1 6461F53B
P 5650 4200
AR Path="/6461F53B" Ref="#PWR?"  Part="1" 
AR Path="/645F0C2C/6461F53B" Ref="#PWR0139"  Part="1" 
F 0 "#PWR0139" H 5650 4050 50  0001 C CNN
F 1 "+5V" V 5665 4328 50  0000 L CNN
F 2 "" H 5650 4200 50  0001 C CNN
F 3 "" H 5650 4200 50  0001 C CNN
	1    5650 4200
	0    1    1    0   
$EndComp
Text GLabel 5200 4200 0    50   Input ~ 0
5V_RAW
Text GLabel 2200 1950 1    50   Input ~ 0
5V_RAW
Text GLabel 3050 3750 2    50   Input ~ 0
5V_RAW
Wire Wire Line
	2750 3750 3050 3750
Wire Wire Line
	5200 4200 5350 4200
Wire Wire Line
	5550 4200 5650 4200
Text Notes 2250 4800 0    50   ~ 0
Keep resistors close\nto RP2040 pins.
Wire Wire Line
	1400 2050 1500 2050
Wire Wire Line
	1400 2150 1550 2150
Wire Wire Line
	1550 2150 1550 2400
$Comp
L Connector_Generic:Conn_01x03 J7
U 1 1 64DA41BE
P 1200 2050
F 0 "J7" H 1118 1725 50  0000 C CNN
F 1 "POWER_IN" H 1118 1816 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Horizontal" H 1200 2050 50  0001 C CNN
F 3 "~" H 1200 2050 50  0001 C CNN
	1    1200 2050
	-1   0    0    1   
$EndComp
Text GLabel 2250 4250 1    50   Input ~ 0
USB_CONN_D-
Text GLabel 2200 4450 1    50   Input ~ 0
USB_CONN_D+
$EndSCHEMATC
