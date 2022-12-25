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
L Device:D_Bridge_+AA- D1
U 1 1 639AA6E3
P 5150 2550
F 0 "D1" H 5450 2800 50  0000 L CNN
F 1 "D_Bridge_+AA-" H 5450 2700 50  0000 L CNN
F 2 "cd-hd2004:CD-HD2004" H 5150 2550 50  0001 C CNN
F 3 "~" H 5150 2550 50  0001 C CNN
	1    5150 2550
	1    0    0    -1  
$EndComp
$Comp
L Device:D_Bridge_+AA- D2
U 1 1 639AAAC7
P 5150 3850
F 0 "D2" H 5500 4150 50  0000 L CNN
F 1 "D_Bridge_+AA-" H 5500 4050 50  0000 L CNN
F 2 "cd-hd2004:CD-HD2004" H 5150 3850 50  0001 C CNN
F 3 "~" H 5150 3850 50  0001 C CNN
	1    5150 3850
	1    0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x04 J2
U 1 1 639AB638
P 3600 3300
F 0 "J2" H 3518 2875 50  0000 C CNN
F 1 "Conn_01x04" H 3518 2966 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical" H 3600 3300 50  0001 C CNN
F 3 "~" H 3600 3300 50  0001 C CNN
	1    3600 3300
	-1   0    0    1   
$EndComp
$Comp
L Connector_Generic:Conn_01x02 J1
U 1 1 639AC614
P 6750 3050
F 0 "J1" H 6830 3042 50  0000 L CNN
F 1 "Conn_01x02" H 6830 2951 50  0000 L CNN
F 2 "Connector_PinSocket_2.54mm:PinSocket_1x02_P2.54mm_Horizontal" H 6750 3050 50  0001 C CNN
F 3 "~" H 6750 3050 50  0001 C CNN
	1    6750 3050
	1    0    0    -1  
$EndComp
Text GLabel 6550 3150 0    50   Input ~ 0
POE+
Text GLabel 6550 3050 0    50   Input ~ 0
POE-
Text GLabel 5450 2550 2    50   Input ~ 0
POE+
Text GLabel 5450 3850 2    50   Input ~ 0
POE+
Text GLabel 4850 3850 0    50   Input ~ 0
POE-
Text GLabel 4850 2550 0    50   Input ~ 0
POE-
Wire Wire Line
	4250 2050 5150 2050
Wire Wire Line
	5150 2050 5150 2250
Wire Wire Line
	4450 4300 5150 4300
Wire Wire Line
	5150 4300 5150 4150
Wire Wire Line
	4250 3200 3800 3200
Wire Wire Line
	4250 2050 4250 3200
Wire Wire Line
	3800 3100 5150 3100
Wire Wire Line
	5150 3100 5150 2850
Wire Wire Line
	4450 3300 3800 3300
Wire Wire Line
	4450 3300 4450 4300
Wire Wire Line
	3800 3400 5150 3400
Wire Wire Line
	5150 3400 5150 3550
$EndSCHEMATC
