EESchema Schematic File Version 4
EELAYER 30 0
EELAYER END
$Descr A4 11693 8268
encoding utf-8
Sheet 1 1
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
L Diode:1N4004 D1
U 1 1 620768F3
P 1750 2950
F 0 "D1" H 1750 3166 50  0000 C CNN
F 1 "1N4004" H 1750 3075 50  0000 C CNN
F 2 "Diode_THT:D_DO-41_SOD81_P10.16mm_Horizontal" H 1750 2775 50  0001 C CNN
F 3 "http://www.vishay.com/docs/88503/1n4001.pdf" H 1750 2950 50  0001 C CNN
	1    1750 2950
	1    0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x03 J2
U 1 1 62077E72
P 1050 3050
F 0 "J2" H 968 3367 50  0000 C CNN
F 1 "NS_BATTERY" H 968 3276 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical" H 1050 3050 50  0001 C CNN
F 3 "~" H 1050 3050 50  0001 C CNN
	1    1050 3050
	-1   0    0    -1  
$EndComp
$Comp
L Device:R_Small R1
U 1 1 620787E5
P 1800 3250
F 0 "R1" H 1859 3296 50  0000 L CNN
F 1 "10K" H 1859 3205 50  0000 L CNN
F 2 "Resistor_THT:R_Axial_DIN0309_L9.0mm_D3.2mm_P12.70mm_Horizontal" H 1800 3250 50  0001 C CNN
F 3 "~" H 1800 3250 50  0001 C CNN
	1    1800 3250
	1    0    0    -1  
$EndComp
$Comp
L power:+5V #PWR0101
U 1 1 62078C0C
P 1950 2600
F 0 "#PWR0101" H 1950 2450 50  0001 C CNN
F 1 "+5V" H 1965 2773 50  0000 C CNN
F 2 "" H 1950 2600 50  0001 C CNN
F 3 "" H 1950 2600 50  0001 C CNN
	1    1950 2600
	1    0    0    -1  
$EndComp
Wire Wire Line
	1600 2950 1250 2950
Wire Wire Line
	1950 2600 1950 2950
Wire Wire Line
	1950 2950 1900 2950
$Comp
L power:GND #PWR0102
U 1 1 6207AB6C
P 5600 2050
F 0 "#PWR0102" H 5600 1800 50  0001 C CNN
F 1 "GND" H 5605 1877 50  0000 C CNN
F 2 "" H 5600 2050 50  0001 C CNN
F 3 "" H 5600 2050 50  0001 C CNN
	1    5600 2050
	1    0    0    -1  
$EndComp
Wire Wire Line
	1250 3150 1500 3150
Wire Wire Line
	1500 3150 1500 3400
Wire Wire Line
	1250 3050 1800 3050
Wire Wire Line
	1800 3050 1800 3150
Wire Wire Line
	1800 3350 1800 3400
Wire Wire Line
	1800 3400 1500 3400
Connection ~ 1500 3400
$Comp
L Regulator_Linear:L7805 U1
U 1 1 6207C53E
P 4000 2900
F 0 "U1" H 4000 3142 50  0000 C CNN
F 1 "L7805" H 4000 3051 50  0000 C CNN
F 2 "Package_TO_SOT_THT:TO-220-3_Vertical" H 4025 2750 50  0001 L CIN
F 3 "http://www.st.com/content/ccc/resource/technical/document/datasheet/41/4f/b3/b0/12/d4/47/88/CD00000444.pdf/files/CD00000444.pdf/jcr:content/translations/en.CD00000444.pdf" H 4000 2850 50  0001 C CNN
	1    4000 2900
	1    0    0    -1  
$EndComp
$Comp
L Transistor_FET:IRLZ34N Q1
U 1 1 6207ED97
P 5500 1450
F 0 "Q1" H 5705 1496 50  0000 L CNN
F 1 "FQP30N06L" H 5705 1405 50  0000 L CNN
F 2 "Package_TO_SOT_THT:TO-220-3_Vertical" H 5750 1375 50  0001 L CIN
F 3 "http://www.infineon.com/dgdl/irlz34npbf.pdf?fileId=5546d462533600a40153567206892720" H 5500 1450 50  0001 L CNN
	1    5500 1450
	1    0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x06 J1
U 1 1 6207FDF9
P 1050 1300
F 0 "J1" H 968 1717 50  0000 C CNN
F 1 "USBC_IN" H 968 1626 50  0000 C CNN
F 2 "Components:USB_C_BREAKOUT" H 1050 1300 50  0001 C CNN
F 3 "~" H 1050 1300 50  0001 C CNN
	1    1050 1300
	-1   0    0    -1  
$EndComp
Text GLabel 1250 1100 2    50   Input ~ 0
USBC_VBUS
Text GLabel 1250 1200 2    50   Input ~ 0
USBC_IN_GND
Text GLabel 1250 1300 2    50   Input ~ 0
USBC_CC1
Text GLabel 1250 1600 2    50   Input ~ 0
USBC_CC2
NoConn ~ 1250 1400
NoConn ~ 1250 1500
$Comp
L Connector_Generic:Conn_01x06 J3
U 1 1 6208AFD1
P 2550 1300
F 0 "J3" H 2468 1717 50  0000 C CNN
F 1 "USBC_OUT" H 2468 1626 50  0000 C CNN
F 2 "Components:USB_C_BREAKOUT" H 2550 1300 50  0001 C CNN
F 3 "~" H 2550 1300 50  0001 C CNN
	1    2550 1300
	-1   0    0    -1  
$EndComp
Text GLabel 2750 1100 2    50   Input ~ 0
USBC_VBUS
Text GLabel 2750 1300 2    50   Input ~ 0
USBC_CC1
Text GLabel 2750 1600 2    50   Input ~ 0
USBC_CC2
NoConn ~ 2750 1400
NoConn ~ 2750 1500
Text GLabel 2750 1200 2    50   Input ~ 0
USBC_OUT_GND
Text Notes 950  2600 0    50   ~ 0
Pin 1: Battery V+\nPin 2: Temp\nPin 3: Battery GND
Text GLabel 1550 3650 2    50   Input ~ 0
BATTERY_GND
Wire Wire Line
	1500 3650 1550 3650
Wire Wire Line
	1500 3400 1500 3650
Text Notes 950  750  0    50   ~ 0
Input from USB PSU\nVBus is either 5V or 14.5V
Text Notes 3850 700  2    50   ~ 0
Output power to Nintendo Switch dock
$Comp
L Device:R_Small R3
U 1 1 620A22E1
P 5100 1700
F 0 "R3" H 5159 1746 50  0000 L CNN
F 1 "10K" H 5159 1655 50  0000 L CNN
F 2 "Resistor_THT:R_Axial_DIN0309_L9.0mm_D3.2mm_P12.70mm_Horizontal" H 5100 1700 50  0001 C CNN
F 3 "~" H 5100 1700 50  0001 C CNN
	1    5100 1700
	1    0    0    -1  
$EndComp
$Comp
L Device:R_Small R2
U 1 1 620A2778
P 4850 1450
F 0 "R2" V 4654 1450 50  0000 C CNN
F 1 "10K" V 4745 1450 50  0000 C CNN
F 2 "Resistor_THT:R_Axial_DIN0309_L9.0mm_D3.2mm_P12.70mm_Horizontal" H 4850 1450 50  0001 C CNN
F 3 "~" H 4850 1450 50  0001 C CNN
	1    4850 1450
	0    1    1    0   
$EndComp
Wire Wire Line
	4950 1450 5100 1450
Wire Wire Line
	5100 1600 5100 1450
Connection ~ 5100 1450
Wire Wire Line
	5100 1450 5300 1450
Text GLabel 5350 850  0    50   Input ~ 0
USBC_OUT_GND
Text GLabel 1350 1900 0    50   Input ~ 0
USBC_IN_GND
$Comp
L power:GND #PWR0103
U 1 1 620AAFF4
P 1650 1900
F 0 "#PWR0103" H 1650 1650 50  0001 C CNN
F 1 "GND" V 1655 1772 50  0000 R CNN
F 2 "" H 1650 1900 50  0001 C CNN
F 3 "" H 1650 1900 50  0001 C CNN
	1    1650 1900
	0    -1   -1   0   
$EndComp
Wire Wire Line
	1650 1900 1350 1900
$Comp
L power:GND #PWR0104
U 1 1 620ABA1D
P 4000 3300
F 0 "#PWR0104" H 4000 3050 50  0001 C CNN
F 1 "GND" H 4005 3127 50  0000 C CNN
F 2 "" H 4000 3300 50  0001 C CNN
F 3 "" H 4000 3300 50  0001 C CNN
	1    4000 3300
	1    0    0    -1  
$EndComp
Wire Wire Line
	4000 3200 4000 3300
Text GLabel 3600 2900 0    50   Input ~ 0
USBC_VBUS
Wire Wire Line
	3600 2900 3700 2900
$Comp
L power:+5V #PWR0105
U 1 1 620ACCEE
P 4400 2900
F 0 "#PWR0105" H 4400 2750 50  0001 C CNN
F 1 "+5V" V 4415 3028 50  0000 L CNN
F 2 "" H 4400 2900 50  0001 C CNN
F 3 "" H 4400 2900 50  0001 C CNN
	1    4400 2900
	0    1    1    0   
$EndComp
Wire Wire Line
	4400 2900 4300 2900
Text GLabel 5350 1050 0    50   Input ~ 0
BATTERY_GND
Wire Wire Line
	5600 1250 5600 1050
Wire Wire Line
	5350 850  5600 850 
Wire Wire Line
	5600 1650 5600 1900
Wire Wire Line
	5100 1800 5100 1900
Wire Wire Line
	5100 1900 5600 1900
Connection ~ 5600 1900
Wire Wire Line
	5600 1900 5600 2050
Text Notes 2050 3350 0    50   ~ 0
Simulates 10K\nthermistor
$Comp
L Connector_Generic:Conn_01x03 J4
U 1 1 620B7C7D
P 5400 3150
F 0 "J4" H 5318 3467 50  0000 C CNN
F 1 "SERVO" H 5318 3376 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical" H 5400 3150 50  0001 C CNN
F 3 "~" H 5400 3150 50  0001 C CNN
	1    5400 3150
	-1   0    0    -1  
$EndComp
$Comp
L power:+5V #PWR0106
U 1 1 620B989C
P 5800 3150
F 0 "#PWR0106" H 5800 3000 50  0001 C CNN
F 1 "+5V" V 5815 3278 50  0000 L CNN
F 2 "" H 5800 3150 50  0001 C CNN
F 3 "" H 5800 3150 50  0001 C CNN
	1    5800 3150
	0    1    1    0   
$EndComp
Wire Wire Line
	5800 3150 5600 3150
$Comp
L power:GND #PWR0107
U 1 1 620BA402
P 5800 3250
F 0 "#PWR0107" H 5800 3000 50  0001 C CNN
F 1 "GND" V 5805 3122 50  0000 R CNN
F 2 "" H 5800 3250 50  0001 C CNN
F 3 "" H 5800 3250 50  0001 C CNN
	1    5800 3250
	0    -1   -1   0   
$EndComp
Wire Wire Line
	5800 3250 5600 3250
Text GLabel 5800 3050 2    50   Input ~ 0
SERVO_PWM
Wire Wire Line
	5800 3050 5600 3050
Text GLabel 4700 1450 0    50   Input ~ 0
POWER_ENABLE
Wire Wire Line
	4750 1450 4700 1450
Wire Wire Line
	5350 1050 5600 1050
Connection ~ 5600 1050
Wire Wire Line
	5600 1050 5600 850 
$Comp
L Components:TINY2040 U2
U 1 1 620CA23F
P 7700 1400
F 0 "U2" H 7700 1515 50  0000 C CNN
F 1 "TINY2040" H 7700 1424 50  0000 C CNN
F 2 "Components:TINY2040_TH" H 7700 1400 50  0001 C CNN
F 3 "" H 7700 1400 50  0001 C CNN
	1    7700 1400
	1    0    0    -1  
$EndComp
$Comp
L power:+5V #PWR0108
U 1 1 620D0CCA
P 7200 1550
F 0 "#PWR0108" H 7200 1400 50  0001 C CNN
F 1 "+5V" V 7215 1678 50  0000 L CNN
F 2 "" H 7200 1550 50  0001 C CNN
F 3 "" H 7200 1550 50  0001 C CNN
	1    7200 1550
	0    -1   -1   0   
$EndComp
Wire Wire Line
	7200 1550 7350 1550
$Comp
L power:GND #PWR0109
U 1 1 620D2A6E
P 7200 1650
F 0 "#PWR0109" H 7200 1400 50  0001 C CNN
F 1 "GND" V 7205 1522 50  0000 R CNN
F 2 "" H 7200 1650 50  0001 C CNN
F 3 "" H 7200 1650 50  0001 C CNN
	1    7200 1650
	0    1    1    0   
$EndComp
Wire Wire Line
	7200 1650 7350 1650
$Comp
L power:GND #PWR0110
U 1 1 620D3C6F
P 7200 2250
F 0 "#PWR0110" H 7200 2000 50  0001 C CNN
F 1 "GND" V 7205 2122 50  0000 R CNN
F 2 "" H 7200 2250 50  0001 C CNN
F 3 "" H 7200 2250 50  0001 C CNN
	1    7200 2250
	0    1    1    0   
$EndComp
Wire Wire Line
	7200 2250 7350 2250
Text GLabel 7350 2050 0    50   Input ~ 0
SERVO_PWM
Text GLabel 7350 2150 0    50   Input ~ 0
POWER_ENABLE
$EndSCHEMATC