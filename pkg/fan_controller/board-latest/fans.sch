EESchema Schematic File Version 4
EELAYER 30 0
EELAYER END
$Descr A4 11693 8268
encoding utf-8
Sheet 2 4
Title ""
Date ""
Rev ""
Comp ""
Comment1 ""
Comment2 ""
Comment3 ""
Comment4 ""
$EndDescr
Text GLabel 1600 1400 2    50   Input ~ 0
FAN_PWM_RAW_1
Wire Wire Line
	1800 1700 1600 1700
$Comp
L power:GND #PWR?
U 1 1 64560EE1
P 1800 1700
AR Path="/64560EE1" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560EE1" Ref="#PWR0101"  Part="1" 
F 0 "#PWR0101" H 1800 1450 50  0001 C CNN
F 1 "GND" V 1805 1572 50  0000 R CNN
F 2 "" H 1800 1700 50  0001 C CNN
F 3 "" H 1800 1700 50  0001 C CNN
	1    1800 1700
	0    -1   -1   0   
$EndComp
Text GLabel 1600 1500 2    50   Input ~ 0
FAN_TACH_RAW_1
$Comp
L power:+12V #PWR?
U 1 1 64560EE8
P 2400 1600
AR Path="/64560EE8" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560EE8" Ref="#PWR0102"  Part="1" 
F 0 "#PWR0102" H 2400 1450 50  0001 C CNN
F 1 "+12V" V 2415 1728 50  0000 L CNN
F 2 "" H 2400 1600 50  0001 C CNN
F 3 "" H 2400 1600 50  0001 C CNN
	1    2400 1600
	0    1    1    0   
$EndComp
$Comp
L Connector_Generic:Conn_01x04 J?
U 1 1 64560EEE
P 1400 1500
AR Path="/64560EEE" Ref="J?"  Part="1" 
AR Path="/644730C0/64560EEE" Ref="J1"  Part="1" 
F 0 "J1" H 1318 1817 50  0000 C CNN
F 1 "FAN1" H 1318 1726 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Horizontal" H 1400 1500 50  0001 C CNN
F 3 "~" H 1400 1500 50  0001 C CNN
	1    1400 1500
	-1   0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x04 J?
U 1 1 64560EF4
P 1400 2300
AR Path="/64560EF4" Ref="J?"  Part="1" 
AR Path="/644730C0/64560EF4" Ref="J2"  Part="1" 
F 0 "J2" H 1318 2617 50  0000 C CNN
F 1 "FAN2" H 1318 2526 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Horizontal" H 1400 2300 50  0001 C CNN
F 3 "~" H 1400 2300 50  0001 C CNN
	1    1400 2300
	-1   0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x04 J?
U 1 1 64560EFA
P 1400 3150
AR Path="/64560EFA" Ref="J?"  Part="1" 
AR Path="/644730C0/64560EFA" Ref="J3"  Part="1" 
F 0 "J3" H 1318 3467 50  0000 C CNN
F 1 "FAN3" H 1318 3376 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Horizontal" H 1400 3150 50  0001 C CNN
F 3 "~" H 1400 3150 50  0001 C CNN
	1    1400 3150
	-1   0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x04 J?
U 1 1 64560F00
P 1400 4150
AR Path="/64560F00" Ref="J?"  Part="1" 
AR Path="/644730C0/64560F00" Ref="J4"  Part="1" 
F 0 "J4" H 1318 4467 50  0000 C CNN
F 1 "FAN4" H 1318 4376 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Horizontal" H 1400 4150 50  0001 C CNN
F 3 "~" H 1400 4150 50  0001 C CNN
	1    1400 4150
	-1   0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x04 J?
U 1 1 64560F06
P 1400 6100
AR Path="/64560F06" Ref="J?"  Part="1" 
AR Path="/644730C0/64560F06" Ref="J6"  Part="1" 
F 0 "J6" H 1318 6417 50  0000 C CNN
F 1 "PUMP" H 1318 6326 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Horizontal" H 1400 6100 50  0001 C CNN
F 3 "~" H 1400 6100 50  0001 C CNN
	1    1400 6100
	-1   0    0    -1  
$EndComp
$Comp
L Transistor_FET:2N7002 Q?
U 1 1 64560F0C
P 4250 1600
AR Path="/64560F0C" Ref="Q?"  Part="1" 
AR Path="/644730C0/64560F0C" Ref="Q1"  Part="1" 
F 0 "Q1" H 4454 1646 50  0000 L CNN
F 1 "2N7002" H 4454 1555 50  0000 L CNN
F 2 "Package_TO_SOT_SMD:SOT-23" H 4450 1525 50  0001 L CIN
F 3 "https://www.fairchildsemi.com/datasheets/2N/2N7002.pdf" H 4250 1600 50  0001 L CNN
	1    4250 1600
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64560F13
P 3850 1600
AR Path="/64560F13" Ref="R?"  Part="1" 
AR Path="/644730C0/64560F13" Ref="R3"  Part="1" 
F 0 "R3" V 3950 1500 50  0000 L CNN
F 1 "100" V 4050 1500 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 1600 50  0001 C CNN
F 3 "~" H 3850 1600 50  0001 C CNN
F 4 "C21190" H 3850 1600 50  0001 C CNN "LCSC"
	1    3850 1600
	0    1    1    0   
$EndComp
Text GLabel 3600 1600 0    50   Input ~ 0
FAN_PWM_1
Text GLabel 3600 1300 0    50   Input ~ 0
FAN_PWM_RAW_1
$Comp
L power:GND #PWR?
U 1 1 64560F1B
P 4350 1950
AR Path="/64560F1B" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F1B" Ref="#PWR0103"  Part="1" 
F 0 "#PWR0103" H 4350 1700 50  0001 C CNN
F 1 "GND" V 4355 1822 50  0000 R CNN
F 2 "" H 4350 1950 50  0001 C CNN
F 3 "" H 4350 1950 50  0001 C CNN
	1    4350 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	4350 1800 4350 1950
Wire Wire Line
	3600 1600 3750 1600
Wire Wire Line
	3950 1600 4050 1600
Wire Wire Line
	4350 1300 4350 1400
Wire Wire Line
	1600 1600 2400 1600
Wire Wire Line
	3600 1300 3750 1300
Wire Wire Line
	3950 1300 4350 1300
Text GLabel 6350 1400 0    50   Input ~ 0
FAN_TACH_RAW_1
$Comp
L Device:R_Small R?
U 1 1 64560F2A
P 3850 1300
AR Path="/64560F2A" Ref="R?"  Part="1" 
AR Path="/644730C0/64560F2A" Ref="R1"  Part="1" 
F 0 "R1" V 4050 1250 50  0000 L CNN
F 1 "470" V 3950 1250 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 1300 50  0001 C CNN
F 3 "~" H 3850 1300 50  0001 C CNN
F 4 "C23204" H 3850 1300 50  0001 C CNN "LCSC"
	1    3850 1300
	0    -1   -1   0   
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64560F31
P 6550 1400
AR Path="/64560F31" Ref="R?"  Part="1" 
AR Path="/644730C0/64560F31" Ref="R2"  Part="1" 
F 0 "R2" V 6750 1400 50  0000 L CNN
F 1 "470" V 6650 1350 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 6550 1400 50  0001 C CNN
F 3 "~" H 6550 1400 50  0001 C CNN
F 4 "C23204" H 6550 1400 50  0001 C CNN "LCSC"
	1    6550 1400
	0    -1   -1   0   
$EndComp
Text GLabel 6750 1400 2    50   Input ~ 0
FAN_TACH_1
Wire Wire Line
	6450 1400 6350 1400
Wire Wire Line
	6750 1400 6650 1400
Text Notes 5650 1050 0    50   ~ 0
Fan tachometer inputs should be\npulled up to 3V3 in the MCU.
Text GLabel 1600 2200 2    50   Input ~ 0
FAN_PWM_RAW_2
Wire Wire Line
	1800 2500 1600 2500
$Comp
L power:GND #PWR?
U 1 1 64560F3D
P 1800 2500
AR Path="/64560F3D" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F3D" Ref="#PWR0104"  Part="1" 
F 0 "#PWR0104" H 1800 2250 50  0001 C CNN
F 1 "GND" V 1805 2372 50  0000 R CNN
F 2 "" H 1800 2500 50  0001 C CNN
F 3 "" H 1800 2500 50  0001 C CNN
	1    1800 2500
	0    -1   -1   0   
$EndComp
Text GLabel 1600 2300 2    50   Input ~ 0
FAN_TACH_RAW_2
$Comp
L power:+12V #PWR?
U 1 1 64560F44
P 2400 2400
AR Path="/64560F44" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F44" Ref="#PWR0105"  Part="1" 
F 0 "#PWR0105" H 2400 2250 50  0001 C CNN
F 1 "+12V" V 2415 2528 50  0000 L CNN
F 2 "" H 2400 2400 50  0001 C CNN
F 3 "" H 2400 2400 50  0001 C CNN
	1    2400 2400
	0    1    1    0   
$EndComp
Wire Wire Line
	1600 2400 2400 2400
Text GLabel 1600 3050 2    50   Input ~ 0
FAN_PWM_RAW_3
Wire Wire Line
	1800 3350 1600 3350
$Comp
L power:GND #PWR?
U 1 1 64560F4D
P 1800 3350
AR Path="/64560F4D" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F4D" Ref="#PWR0106"  Part="1" 
F 0 "#PWR0106" H 1800 3100 50  0001 C CNN
F 1 "GND" V 1805 3222 50  0000 R CNN
F 2 "" H 1800 3350 50  0001 C CNN
F 3 "" H 1800 3350 50  0001 C CNN
	1    1800 3350
	0    -1   -1   0   
$EndComp
Text GLabel 1600 3150 2    50   Input ~ 0
FAN_TACH_RAW_3
$Comp
L power:+12V #PWR?
U 1 1 64560F54
P 2400 3250
AR Path="/64560F54" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F54" Ref="#PWR0107"  Part="1" 
F 0 "#PWR0107" H 2400 3100 50  0001 C CNN
F 1 "+12V" V 2415 3378 50  0000 L CNN
F 2 "" H 2400 3250 50  0001 C CNN
F 3 "" H 2400 3250 50  0001 C CNN
	1    2400 3250
	0    1    1    0   
$EndComp
Wire Wire Line
	1600 3250 2400 3250
Text GLabel 1600 4050 2    50   Input ~ 0
FAN_PWM_RAW_4
Wire Wire Line
	1800 4350 1600 4350
$Comp
L power:GND #PWR?
U 1 1 64560F5D
P 1800 4350
AR Path="/64560F5D" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F5D" Ref="#PWR0108"  Part="1" 
F 0 "#PWR0108" H 1800 4100 50  0001 C CNN
F 1 "GND" V 1805 4222 50  0000 R CNN
F 2 "" H 1800 4350 50  0001 C CNN
F 3 "" H 1800 4350 50  0001 C CNN
	1    1800 4350
	0    -1   -1   0   
$EndComp
Text GLabel 1600 4150 2    50   Input ~ 0
FAN_TACH_RAW_4
$Comp
L power:+12V #PWR?
U 1 1 64560F64
P 2400 4250
AR Path="/64560F64" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F64" Ref="#PWR0109"  Part="1" 
F 0 "#PWR0109" H 2400 4100 50  0001 C CNN
F 1 "+12V" V 2415 4378 50  0000 L CNN
F 2 "" H 2400 4250 50  0001 C CNN
F 3 "" H 2400 4250 50  0001 C CNN
	1    2400 4250
	0    1    1    0   
$EndComp
Wire Wire Line
	1600 4250 2400 4250
Text GLabel 1600 6000 2    50   Input ~ 0
FAN_PWM_RAW_6
Wire Wire Line
	1800 6300 1600 6300
$Comp
L power:GND #PWR?
U 1 1 64560F6D
P 1800 6300
AR Path="/64560F6D" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F6D" Ref="#PWR0110"  Part="1" 
F 0 "#PWR0110" H 1800 6050 50  0001 C CNN
F 1 "GND" V 1805 6172 50  0000 R CNN
F 2 "" H 1800 6300 50  0001 C CNN
F 3 "" H 1800 6300 50  0001 C CNN
	1    1800 6300
	0    -1   -1   0   
$EndComp
Text GLabel 1600 6100 2    50   Input ~ 0
FAN_TACH_RAW_6
$Comp
L power:+12V #PWR?
U 1 1 64560F74
P 2400 6200
AR Path="/64560F74" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F74" Ref="#PWR0111"  Part="1" 
F 0 "#PWR0111" H 2400 6050 50  0001 C CNN
F 1 "+12V" V 2415 6328 50  0000 L CNN
F 2 "" H 2400 6200 50  0001 C CNN
F 3 "" H 2400 6200 50  0001 C CNN
	1    2400 6200
	0    1    1    0   
$EndComp
Wire Wire Line
	1600 6200 2400 6200
$Comp
L Transistor_FET:2N7002 Q?
U 1 1 64560F7B
P 4250 2700
AR Path="/64560F7B" Ref="Q?"  Part="1" 
AR Path="/644730C0/64560F7B" Ref="Q3"  Part="1" 
F 0 "Q3" H 4454 2746 50  0000 L CNN
F 1 "2N7002" H 4454 2655 50  0000 L CNN
F 2 "Package_TO_SOT_SMD:SOT-23" H 4450 2625 50  0001 L CIN
F 3 "https://www.fairchildsemi.com/datasheets/2N/2N7002.pdf" H 4250 2700 50  0001 L CNN
	1    4250 2700
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64560F82
P 3850 2700
AR Path="/64560F82" Ref="R?"  Part="1" 
AR Path="/644730C0/64560F82" Ref="R17"  Part="1" 
F 0 "R17" V 3950 2600 50  0000 L CNN
F 1 "100" V 4050 2600 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 2700 50  0001 C CNN
F 3 "~" H 3850 2700 50  0001 C CNN
F 4 "C21190" H 3850 2700 50  0001 C CNN "LCSC"
	1    3850 2700
	0    1    1    0   
$EndComp
Text GLabel 3600 2700 0    50   Input ~ 0
FAN_PWM_2
Text GLabel 3600 2400 0    50   Input ~ 0
FAN_PWM_RAW_2
$Comp
L power:GND #PWR?
U 1 1 64560F8A
P 4350 3050
AR Path="/64560F8A" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560F8A" Ref="#PWR0112"  Part="1" 
F 0 "#PWR0112" H 4350 2800 50  0001 C CNN
F 1 "GND" V 4355 2922 50  0000 R CNN
F 2 "" H 4350 3050 50  0001 C CNN
F 3 "" H 4350 3050 50  0001 C CNN
	1    4350 3050
	1    0    0    -1  
$EndComp
Wire Wire Line
	4350 2900 4350 3050
Wire Wire Line
	3600 2700 3750 2700
Wire Wire Line
	3950 2700 4050 2700
Wire Wire Line
	4350 2400 4350 2500
Wire Wire Line
	3600 2400 3750 2400
Wire Wire Line
	3950 2400 4350 2400
$Comp
L Device:R_Small R?
U 1 1 64560F97
P 3850 2400
AR Path="/64560F97" Ref="R?"  Part="1" 
AR Path="/644730C0/64560F97" Ref="R6"  Part="1" 
F 0 "R6" V 4050 2350 50  0000 L CNN
F 1 "470" V 3950 2350 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 2400 50  0001 C CNN
F 3 "~" H 3850 2400 50  0001 C CNN
F 4 "C23204" H 3850 2400 50  0001 C CNN "LCSC"
	1    3850 2400
	0    -1   -1   0   
$EndComp
$Comp
L Transistor_FET:2N7002 Q?
U 1 1 64560F9D
P 4250 3750
AR Path="/64560F9D" Ref="Q?"  Part="1" 
AR Path="/644730C0/64560F9D" Ref="Q5"  Part="1" 
F 0 "Q5" H 4454 3796 50  0000 L CNN
F 1 "2N7002" H 4454 3705 50  0000 L CNN
F 2 "Package_TO_SOT_SMD:SOT-23" H 4450 3675 50  0001 L CIN
F 3 "https://www.fairchildsemi.com/datasheets/2N/2N7002.pdf" H 4250 3750 50  0001 L CNN
	1    4250 3750
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64560FA4
P 3850 3750
AR Path="/64560FA4" Ref="R?"  Part="1" 
AR Path="/644730C0/64560FA4" Ref="R21"  Part="1" 
F 0 "R21" V 3950 3650 50  0000 L CNN
F 1 "100" V 4050 3650 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 3750 50  0001 C CNN
F 3 "~" H 3850 3750 50  0001 C CNN
F 4 "C21190" H 3850 3750 50  0001 C CNN "LCSC"
	1    3850 3750
	0    1    1    0   
$EndComp
Text GLabel 3600 3750 0    50   Input ~ 0
FAN_PWM_3
Text GLabel 3600 3450 0    50   Input ~ 0
FAN_PWM_RAW_3
$Comp
L power:GND #PWR?
U 1 1 64560FAC
P 4350 4100
AR Path="/64560FAC" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560FAC" Ref="#PWR0113"  Part="1" 
F 0 "#PWR0113" H 4350 3850 50  0001 C CNN
F 1 "GND" V 4355 3972 50  0000 R CNN
F 2 "" H 4350 4100 50  0001 C CNN
F 3 "" H 4350 4100 50  0001 C CNN
	1    4350 4100
	1    0    0    -1  
$EndComp
Wire Wire Line
	4350 3950 4350 4100
Wire Wire Line
	3600 3750 3750 3750
Wire Wire Line
	3950 3750 4050 3750
Wire Wire Line
	4350 3450 4350 3550
Wire Wire Line
	3600 3450 3750 3450
Wire Wire Line
	3950 3450 4350 3450
$Comp
L Device:R_Small R?
U 1 1 64560FB9
P 3850 3450
AR Path="/64560FB9" Ref="R?"  Part="1" 
AR Path="/644730C0/64560FB9" Ref="R20"  Part="1" 
F 0 "R20" V 4050 3400 50  0000 L CNN
F 1 "470" V 3950 3400 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 3450 50  0001 C CNN
F 3 "~" H 3850 3450 50  0001 C CNN
F 4 "C23204" H 3850 3450 50  0001 C CNN "LCSC"
	1    3850 3450
	0    -1   -1   0   
$EndComp
$Comp
L Transistor_FET:2N7002 Q?
U 1 1 64560FBF
P 4250 4800
AR Path="/64560FBF" Ref="Q?"  Part="1" 
AR Path="/644730C0/64560FBF" Ref="Q6"  Part="1" 
F 0 "Q6" H 4454 4846 50  0000 L CNN
F 1 "2N7002" H 4454 4755 50  0000 L CNN
F 2 "Package_TO_SOT_SMD:SOT-23" H 4450 4725 50  0001 L CIN
F 3 "https://www.fairchildsemi.com/datasheets/2N/2N7002.pdf" H 4250 4800 50  0001 L CNN
	1    4250 4800
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64560FC6
P 3850 4800
AR Path="/64560FC6" Ref="R?"  Part="1" 
AR Path="/644730C0/64560FC6" Ref="R23"  Part="1" 
F 0 "R23" V 3950 4700 50  0000 L CNN
F 1 "100" V 4050 4700 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 4800 50  0001 C CNN
F 3 "~" H 3850 4800 50  0001 C CNN
F 4 "C21190" H 3850 4800 50  0001 C CNN "LCSC"
	1    3850 4800
	0    1    1    0   
$EndComp
Text GLabel 3600 4800 0    50   Input ~ 0
FAN_PWM_4
Text GLabel 3600 4500 0    50   Input ~ 0
FAN_PWM_RAW_4
$Comp
L power:GND #PWR?
U 1 1 64560FCE
P 4350 5150
AR Path="/64560FCE" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560FCE" Ref="#PWR0114"  Part="1" 
F 0 "#PWR0114" H 4350 4900 50  0001 C CNN
F 1 "GND" V 4355 5022 50  0000 R CNN
F 2 "" H 4350 5150 50  0001 C CNN
F 3 "" H 4350 5150 50  0001 C CNN
	1    4350 5150
	1    0    0    -1  
$EndComp
Wire Wire Line
	4350 5000 4350 5150
Wire Wire Line
	3600 4800 3750 4800
Wire Wire Line
	3950 4800 4050 4800
Wire Wire Line
	4350 4500 4350 4600
Wire Wire Line
	3600 4500 3750 4500
Wire Wire Line
	3950 4500 4350 4500
$Comp
L Device:R_Small R?
U 1 1 64560FDB
P 3850 4500
AR Path="/64560FDB" Ref="R?"  Part="1" 
AR Path="/644730C0/64560FDB" Ref="R22"  Part="1" 
F 0 "R22" V 4050 4450 50  0000 L CNN
F 1 "470" V 3950 4450 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 4500 50  0001 C CNN
F 3 "~" H 3850 4500 50  0001 C CNN
F 4 "C23204" H 3850 4500 50  0001 C CNN "LCSC"
	1    3850 4500
	0    -1   -1   0   
$EndComp
$Comp
L Transistor_FET:2N7002 Q?
U 1 1 64560FE1
P 4250 5900
AR Path="/64560FE1" Ref="Q?"  Part="1" 
AR Path="/644730C0/64560FE1" Ref="Q7"  Part="1" 
F 0 "Q7" H 4454 5946 50  0000 L CNN
F 1 "2N7002" H 4454 5855 50  0000 L CNN
F 2 "Package_TO_SOT_SMD:SOT-23" H 4450 5825 50  0001 L CIN
F 3 "https://www.fairchildsemi.com/datasheets/2N/2N7002.pdf" H 4250 5900 50  0001 L CNN
	1    4250 5900
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64560FE8
P 3850 5900
AR Path="/64560FE8" Ref="R?"  Part="1" 
AR Path="/644730C0/64560FE8" Ref="R25"  Part="1" 
F 0 "R25" V 3950 5800 50  0000 L CNN
F 1 "100" V 4050 5800 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 5900 50  0001 C CNN
F 3 "~" H 3850 5900 50  0001 C CNN
F 4 "C21190" H 3850 5900 50  0001 C CNN "LCSC"
	1    3850 5900
	0    1    1    0   
$EndComp
Text GLabel 3600 5900 0    50   Input ~ 0
FAN_PWM_5
Text GLabel 3600 5600 0    50   Input ~ 0
FAN_PWM_RAW_5
$Comp
L power:GND #PWR?
U 1 1 64560FF0
P 4350 6250
AR Path="/64560FF0" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64560FF0" Ref="#PWR0115"  Part="1" 
F 0 "#PWR0115" H 4350 6000 50  0001 C CNN
F 1 "GND" V 4355 6122 50  0000 R CNN
F 2 "" H 4350 6250 50  0001 C CNN
F 3 "" H 4350 6250 50  0001 C CNN
	1    4350 6250
	1    0    0    -1  
$EndComp
Wire Wire Line
	4350 6100 4350 6250
Wire Wire Line
	3600 5900 3750 5900
Wire Wire Line
	3950 5900 4050 5900
Wire Wire Line
	4350 5600 4350 5700
Wire Wire Line
	3600 5600 3750 5600
Wire Wire Line
	3950 5600 4350 5600
$Comp
L Device:R_Small R?
U 1 1 64560FFD
P 3850 5600
AR Path="/64560FFD" Ref="R?"  Part="1" 
AR Path="/644730C0/64560FFD" Ref="R24"  Part="1" 
F 0 "R24" V 4050 5550 50  0000 L CNN
F 1 "470" V 3950 5550 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 5600 50  0001 C CNN
F 3 "~" H 3850 5600 50  0001 C CNN
F 4 "C23204" H 3850 5600 50  0001 C CNN "LCSC"
	1    3850 5600
	0    -1   -1   0   
$EndComp
Text GLabel 6350 1800 0    50   Input ~ 0
FAN_TACH_RAW_2
$Comp
L Device:R_Small R?
U 1 1 64561005
P 6550 1800
AR Path="/64561005" Ref="R?"  Part="1" 
AR Path="/644730C0/64561005" Ref="R4"  Part="1" 
F 0 "R4" V 6750 1800 50  0000 L CNN
F 1 "470" V 6650 1750 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 6550 1800 50  0001 C CNN
F 3 "~" H 6550 1800 50  0001 C CNN
F 4 "C23204" H 6550 1800 50  0001 C CNN "LCSC"
	1    6550 1800
	0    -1   -1   0   
$EndComp
Text GLabel 6750 1800 2    50   Input ~ 0
FAN_TACH_2
Wire Wire Line
	6450 1800 6350 1800
Wire Wire Line
	6750 1800 6650 1800
Text GLabel 6350 2150 0    50   Input ~ 0
FAN_TACH_RAW_3
$Comp
L Device:R_Small R?
U 1 1 64561010
P 6550 2150
AR Path="/64561010" Ref="R?"  Part="1" 
AR Path="/644730C0/64561010" Ref="R5"  Part="1" 
F 0 "R5" V 6750 2150 50  0000 L CNN
F 1 "470" V 6650 2100 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 6550 2150 50  0001 C CNN
F 3 "~" H 6550 2150 50  0001 C CNN
F 4 "C23204" H 6550 2150 50  0001 C CNN "LCSC"
	1    6550 2150
	0    -1   -1   0   
$EndComp
Text GLabel 6750 2150 2    50   Input ~ 0
FAN_TACH_3
Wire Wire Line
	6450 2150 6350 2150
Wire Wire Line
	6750 2150 6650 2150
Text GLabel 6350 2500 0    50   Input ~ 0
FAN_TACH_RAW_4
$Comp
L Device:R_Small R?
U 1 1 6456101B
P 6550 2500
AR Path="/6456101B" Ref="R?"  Part="1" 
AR Path="/644730C0/6456101B" Ref="R7"  Part="1" 
F 0 "R7" V 6750 2500 50  0000 L CNN
F 1 "470" V 6650 2450 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 6550 2500 50  0001 C CNN
F 3 "~" H 6550 2500 50  0001 C CNN
F 4 "C23204" H 6550 2500 50  0001 C CNN "LCSC"
	1    6550 2500
	0    -1   -1   0   
$EndComp
Text GLabel 6750 2500 2    50   Input ~ 0
FAN_TACH_4
Wire Wire Line
	6450 2500 6350 2500
Wire Wire Line
	6750 2500 6650 2500
Text GLabel 6350 2850 0    50   Input ~ 0
FAN_TACH_RAW_5
$Comp
L Device:R_Small R?
U 1 1 64561026
P 6550 2850
AR Path="/64561026" Ref="R?"  Part="1" 
AR Path="/644730C0/64561026" Ref="R18"  Part="1" 
F 0 "R18" V 6750 2850 50  0000 L CNN
F 1 "470" V 6650 2800 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 6550 2850 50  0001 C CNN
F 3 "~" H 6550 2850 50  0001 C CNN
F 4 "C23204" H 6550 2850 50  0001 C CNN "LCSC"
	1    6550 2850
	0    -1   -1   0   
$EndComp
Text GLabel 6750 2850 2    50   Input ~ 0
FAN_TACH_5
Wire Wire Line
	6450 2850 6350 2850
Wire Wire Line
	6750 2850 6650 2850
$Comp
L Device:C_Small C?
U 1 1 6458516B
P 8400 1750
AR Path="/6458516B" Ref="C?"  Part="1" 
AR Path="/644730C0/6458516B" Ref="C1"  Part="1" 
F 0 "C1" V 8171 1750 50  0000 C CNN
F 1 "10uF" V 8262 1750 50  0000 C CNN
F 2 "Capacitor_SMD:C_0805_2012Metric_Pad1.15x1.40mm_HandSolder" H 8400 1750 50  0001 C CNN
F 3 "~" H 8400 1750 50  0001 C CNN
F 4 "C15849" H 8400 1750 50  0001 C CNN "LCSC"
	1    8400 1750
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 64585171
P 8400 1950
AR Path="/64585171" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64585171" Ref="#PWR0116"  Part="1" 
F 0 "#PWR0116" H 8400 1700 50  0001 C CNN
F 1 "GND" V 8405 1822 50  0000 R CNN
F 2 "" H 8400 1950 50  0001 C CNN
F 3 "" H 8400 1950 50  0001 C CNN
	1    8400 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	8400 1850 8400 1950
Wire Wire Line
	8400 1650 8400 1600
$Comp
L Device:C_Small C?
U 1 1 6458517A
P 9000 1750
AR Path="/6458517A" Ref="C?"  Part="1" 
AR Path="/644730C0/6458517A" Ref="C2"  Part="1" 
F 0 "C2" V 8771 1750 50  0000 C CNN
F 1 "10uF" V 8862 1750 50  0000 C CNN
F 2 "Capacitor_SMD:C_0805_2012Metric_Pad1.15x1.40mm_HandSolder" H 9000 1750 50  0001 C CNN
F 3 "~" H 9000 1750 50  0001 C CNN
F 4 "C15850" H 9000 1750 50  0001 C CNN "LCSC"
	1    9000 1750
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 64585180
P 9000 1950
AR Path="/64585180" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64585180" Ref="#PWR0117"  Part="1" 
F 0 "#PWR0117" H 9000 1700 50  0001 C CNN
F 1 "GND" V 9005 1822 50  0000 R CNN
F 2 "" H 9000 1950 50  0001 C CNN
F 3 "" H 9000 1950 50  0001 C CNN
	1    9000 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	9000 1850 9000 1950
Wire Wire Line
	9000 1650 9000 1600
$Comp
L Device:C_Small C?
U 1 1 64585189
P 9450 1750
AR Path="/64585189" Ref="C?"  Part="1" 
AR Path="/644730C0/64585189" Ref="C3"  Part="1" 
F 0 "C3" V 9221 1750 50  0000 C CNN
F 1 "10uF" V 9312 1750 50  0000 C CNN
F 2 "Capacitor_SMD:C_0805_2012Metric_Pad1.15x1.40mm_HandSolder" H 9450 1750 50  0001 C CNN
F 3 "~" H 9450 1750 50  0001 C CNN
F 4 "C15850" H 9450 1750 50  0001 C CNN "LCSC"
	1    9450 1750
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 6458518F
P 9450 1950
AR Path="/6458518F" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/6458518F" Ref="#PWR0118"  Part="1" 
F 0 "#PWR0118" H 9450 1700 50  0001 C CNN
F 1 "GND" V 9455 1822 50  0000 R CNN
F 2 "" H 9450 1950 50  0001 C CNN
F 3 "" H 9450 1950 50  0001 C CNN
	1    9450 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	9450 1850 9450 1950
Wire Wire Line
	9450 1650 9450 1600
$Comp
L Device:C_Small C?
U 1 1 64585198
P 10500 1750
AR Path="/64585198" Ref="C?"  Part="1" 
AR Path="/644730C0/64585198" Ref="C5"  Part="1" 
F 0 "C5" V 10271 1750 50  0000 C CNN
F 1 "10uF" V 10362 1750 50  0000 C CNN
F 2 "Capacitor_SMD:C_0805_2012Metric_Pad1.15x1.40mm_HandSolder" H 10500 1750 50  0001 C CNN
F 3 "~" H 10500 1750 50  0001 C CNN
F 4 "C15850" H 10500 1750 50  0001 C CNN "LCSC"
	1    10500 1750
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 6458519E
P 10500 1950
AR Path="/6458519E" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/6458519E" Ref="#PWR0119"  Part="1" 
F 0 "#PWR0119" H 10500 1700 50  0001 C CNN
F 1 "GND" V 10505 1822 50  0000 R CNN
F 2 "" H 10500 1950 50  0001 C CNN
F 3 "" H 10500 1950 50  0001 C CNN
	1    10500 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	10500 1850 10500 1950
Wire Wire Line
	10500 1650 10500 1600
$Comp
L Device:C_Small C?
U 1 1 645851A7
P 10950 1750
AR Path="/645851A7" Ref="C?"  Part="1" 
AR Path="/644730C0/645851A7" Ref="C6"  Part="1" 
F 0 "C6" V 10721 1750 50  0000 C CNN
F 1 "10uF" V 10812 1750 50  0000 C CNN
F 2 "Capacitor_SMD:C_0805_2012Metric_Pad1.15x1.40mm_HandSolder" H 10950 1750 50  0001 C CNN
F 3 "~" H 10950 1750 50  0001 C CNN
F 4 "C15850" H 10950 1750 50  0001 C CNN "LCSC"
	1    10950 1750
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 645851AD
P 10950 1950
AR Path="/645851AD" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/645851AD" Ref="#PWR0120"  Part="1" 
F 0 "#PWR0120" H 10950 1700 50  0001 C CNN
F 1 "GND" V 10955 1822 50  0000 R CNN
F 2 "" H 10950 1950 50  0001 C CNN
F 3 "" H 10950 1950 50  0001 C CNN
	1    10950 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	10950 1850 10950 1950
Wire Wire Line
	10950 1650 10950 1600
$Comp
L power:+12V #PWR?
U 1 1 645851B5
P 9000 1600
AR Path="/645851B5" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/645851B5" Ref="#PWR0121"  Part="1" 
F 0 "#PWR0121" H 9000 1450 50  0001 C CNN
F 1 "+12V" V 9015 1728 50  0000 L CNN
F 2 "" H 9000 1600 50  0001 C CNN
F 3 "" H 9000 1600 50  0001 C CNN
	1    9000 1600
	1    0    0    -1  
$EndComp
$Comp
L power:+12V #PWR?
U 1 1 645851BB
P 9450 1600
AR Path="/645851BB" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/645851BB" Ref="#PWR0122"  Part="1" 
F 0 "#PWR0122" H 9450 1450 50  0001 C CNN
F 1 "+12V" V 9465 1728 50  0000 L CNN
F 2 "" H 9450 1600 50  0001 C CNN
F 3 "" H 9450 1600 50  0001 C CNN
	1    9450 1600
	1    0    0    -1  
$EndComp
$Comp
L power:+12V #PWR?
U 1 1 645851C1
P 10950 1600
AR Path="/645851C1" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/645851C1" Ref="#PWR0123"  Part="1" 
F 0 "#PWR0123" H 10950 1450 50  0001 C CNN
F 1 "+12V" V 10965 1728 50  0000 L CNN
F 2 "" H 10950 1600 50  0001 C CNN
F 3 "" H 10950 1600 50  0001 C CNN
	1    10950 1600
	1    0    0    -1  
$EndComp
$Comp
L power:+12V #PWR?
U 1 1 645851C7
P 10500 1600
AR Path="/645851C7" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/645851C7" Ref="#PWR0124"  Part="1" 
F 0 "#PWR0124" H 10500 1450 50  0001 C CNN
F 1 "+12V" V 10515 1728 50  0000 L CNN
F 2 "" H 10500 1600 50  0001 C CNN
F 3 "" H 10500 1600 50  0001 C CNN
	1    10500 1600
	1    0    0    -1  
$EndComp
$Comp
L power:+12V #PWR?
U 1 1 645851CD
P 8400 1600
AR Path="/645851CD" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/645851CD" Ref="#PWR0125"  Part="1" 
F 0 "#PWR0125" H 8400 1450 50  0001 C CNN
F 1 "+12V" V 8415 1728 50  0000 L CNN
F 2 "" H 8400 1600 50  0001 C CNN
F 3 "" H 8400 1600 50  0001 C CNN
	1    8400 1600
	1    0    0    -1  
$EndComp
Text Notes 2600 900  0    50   ~ 0
FAN_PWM_RAW externally pulled up to 3.3/5V in the fan.
$Comp
L Connector_Generic:Conn_01x04 J?
U 1 1 649E500F
P 1400 5100
AR Path="/649E500F" Ref="J?"  Part="1" 
AR Path="/644730C0/649E500F" Ref="J5"  Part="1" 
F 0 "J5" H 1318 5417 50  0000 C CNN
F 1 "CPU" H 1318 5326 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Horizontal" H 1400 5100 50  0001 C CNN
F 3 "~" H 1400 5100 50  0001 C CNN
	1    1400 5100
	-1   0    0    -1  
$EndComp
$Comp
L Device:C_Small C?
U 1 1 64A219DB
P 9900 1750
AR Path="/64A219DB" Ref="C?"  Part="1" 
AR Path="/644730C0/64A219DB" Ref="C4"  Part="1" 
F 0 "C4" V 9671 1750 50  0000 C CNN
F 1 "10uF" V 9762 1750 50  0000 C CNN
F 2 "Capacitor_SMD:C_0805_2012Metric_Pad1.15x1.40mm_HandSolder" H 9900 1750 50  0001 C CNN
F 3 "~" H 9900 1750 50  0001 C CNN
F 4 "C15850" H 9900 1750 50  0001 C CNN "LCSC"
	1    9900 1750
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 64A219E1
P 9900 1950
AR Path="/64A219E1" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64A219E1" Ref="#PWR0126"  Part="1" 
F 0 "#PWR0126" H 9900 1700 50  0001 C CNN
F 1 "GND" V 9905 1822 50  0000 R CNN
F 2 "" H 9900 1950 50  0001 C CNN
F 3 "" H 9900 1950 50  0001 C CNN
	1    9900 1950
	1    0    0    -1  
$EndComp
Wire Wire Line
	9900 1850 9900 1950
Wire Wire Line
	9900 1650 9900 1600
$Comp
L power:+12V #PWR?
U 1 1 64A219E9
P 9900 1600
AR Path="/64A219E9" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64A219E9" Ref="#PWR0127"  Part="1" 
F 0 "#PWR0127" H 9900 1450 50  0001 C CNN
F 1 "+12V" V 9915 1728 50  0000 L CNN
F 2 "" H 9900 1600 50  0001 C CNN
F 3 "" H 9900 1600 50  0001 C CNN
	1    9900 1600
	1    0    0    -1  
$EndComp
Text GLabel 6350 3200 0    50   Input ~ 0
FAN_TACH_RAW_6
$Comp
L Device:R_Small R?
U 1 1 64A55BA6
P 6550 3200
AR Path="/64A55BA6" Ref="R?"  Part="1" 
AR Path="/644730C0/64A55BA6" Ref="R19"  Part="1" 
F 0 "R19" V 6750 3200 50  0000 L CNN
F 1 "470" V 6650 3150 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 6550 3200 50  0001 C CNN
F 3 "~" H 6550 3200 50  0001 C CNN
F 4 "C23204" H 6550 3200 50  0001 C CNN "LCSC"
	1    6550 3200
	0    -1   -1   0   
$EndComp
Text GLabel 6750 3200 2    50   Input ~ 0
FAN_TACH_6
Wire Wire Line
	6450 3200 6350 3200
Wire Wire Line
	6750 3200 6650 3200
$Comp
L Transistor_FET:2N7002 Q?
U 1 1 64A64271
P 4250 7050
AR Path="/64A64271" Ref="Q?"  Part="1" 
AR Path="/644730C0/64A64271" Ref="Q8"  Part="1" 
F 0 "Q8" H 4454 7096 50  0000 L CNN
F 1 "2N7002" H 4454 7005 50  0000 L CNN
F 2 "Package_TO_SOT_SMD:SOT-23" H 4450 6975 50  0001 L CIN
F 3 "https://www.fairchildsemi.com/datasheets/2N/2N7002.pdf" H 4250 7050 50  0001 L CNN
	1    4250 7050
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R?
U 1 1 64A64278
P 3850 7050
AR Path="/64A64278" Ref="R?"  Part="1" 
AR Path="/644730C0/64A64278" Ref="R27"  Part="1" 
F 0 "R27" V 3950 6950 50  0000 L CNN
F 1 "100" V 4050 6950 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 7050 50  0001 C CNN
F 3 "~" H 3850 7050 50  0001 C CNN
F 4 "C21190" H 3850 7050 50  0001 C CNN "LCSC"
	1    3850 7050
	0    1    1    0   
$EndComp
Text GLabel 3600 7050 0    50   Input ~ 0
FAN_PWM_6
Text GLabel 3600 6750 0    50   Input ~ 0
FAN_PWM_RAW_6
$Comp
L power:GND #PWR?
U 1 1 64A64280
P 4350 7400
AR Path="/64A64280" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64A64280" Ref="#PWR0128"  Part="1" 
F 0 "#PWR0128" H 4350 7150 50  0001 C CNN
F 1 "GND" V 4355 7272 50  0000 R CNN
F 2 "" H 4350 7400 50  0001 C CNN
F 3 "" H 4350 7400 50  0001 C CNN
	1    4350 7400
	1    0    0    -1  
$EndComp
Wire Wire Line
	4350 7250 4350 7400
Wire Wire Line
	3600 7050 3750 7050
Wire Wire Line
	3950 7050 4050 7050
Wire Wire Line
	4350 6750 4350 6850
Wire Wire Line
	3600 6750 3750 6750
Wire Wire Line
	3950 6750 4350 6750
$Comp
L Device:R_Small R?
U 1 1 64A6428D
P 3850 6750
AR Path="/64A6428D" Ref="R?"  Part="1" 
AR Path="/644730C0/64A6428D" Ref="R26"  Part="1" 
F 0 "R26" V 4050 6700 50  0000 L CNN
F 1 "470" V 3950 6700 50  0000 L CNN
F 2 "Resistor_SMD:R_0603_1608Metric" H 3850 6750 50  0001 C CNN
F 3 "~" H 3850 6750 50  0001 C CNN
F 4 "C23204" H 3850 6750 50  0001 C CNN "LCSC"
	1    3850 6750
	0    -1   -1   0   
$EndComp
Wire Wire Line
	1800 5300 1600 5300
$Comp
L power:GND #PWR?
U 1 1 64A93CD9
P 1800 5300
AR Path="/64A93CD9" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64A93CD9" Ref="#PWR0129"  Part="1" 
F 0 "#PWR0129" H 1800 5050 50  0001 C CNN
F 1 "GND" V 1805 5172 50  0000 R CNN
F 2 "" H 1800 5300 50  0001 C CNN
F 3 "" H 1800 5300 50  0001 C CNN
	1    1800 5300
	0    -1   -1   0   
$EndComp
Text GLabel 1600 5100 2    50   Input ~ 0
FAN_TACH_RAW_5
$Comp
L power:+12V #PWR?
U 1 1 64A93CE0
P 2400 5200
AR Path="/64A93CE0" Ref="#PWR?"  Part="1" 
AR Path="/644730C0/64A93CE0" Ref="#PWR0130"  Part="1" 
F 0 "#PWR0130" H 2400 5050 50  0001 C CNN
F 1 "+12V" V 2415 5328 50  0000 L CNN
F 2 "" H 2400 5200 50  0001 C CNN
F 3 "" H 2400 5200 50  0001 C CNN
	1    2400 5200
	0    1    1    0   
$EndComp
Wire Wire Line
	1600 5200 2400 5200
Text GLabel 1600 5000 2    50   Input ~ 0
FAN_PWM_RAW_5
$EndSCHEMATC
