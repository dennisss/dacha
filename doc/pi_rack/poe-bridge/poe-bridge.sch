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
L Connector_Generic:Conn_01x04 J2
U 1 1 63D6AE31
P 4350 3300
F 0 "J2" H 4268 2875 50  0000 C CNN
F 1 "Conn_01x04" H 4268 2966 50  0000 C CNN
F 2 "Connector_Harwin:Harwin_M20-89004xx_1x04_P2.54mm_Horizontal" H 4350 3300 50  0001 C CNN
F 3 "~" H 4350 3300 50  0001 C CNN
	1    4350 3300
	-1   0    0    1   
$EndComp
$Comp
L Connector_Generic:Conn_02x02_Odd_Even J1
U 1 1 63D6BA86
P 5200 3150
F 0 "J1" V 5296 2962 50  0000 R CNN
F 1 "Conn_02x02_Odd_Even" V 5205 2962 50  0000 R CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_2x02_P2.54mm_Vertical" H 5200 3150 50  0001 C CNN
F 3 "~" H 5200 3150 50  0001 C CNN
	1    5200 3150
	0    -1   -1   0   
$EndComp
Wire Wire Line
	4550 3400 5300 3400
Wire Wire Line
	5300 3400 5300 3350
Wire Wire Line
	4550 3300 4950 3300
Wire Wire Line
	4950 3300 4950 3350
Wire Wire Line
	4950 3350 5200 3350
Wire Wire Line
	4550 3200 5000 3200
Wire Wire Line
	5000 3200 5000 2850
Wire Wire Line
	5000 2850 5200 2850
Wire Wire Line
	4550 3100 4900 3100
Wire Wire Line
	4900 3100 4900 2800
Wire Wire Line
	4900 2800 5300 2800
Wire Wire Line
	5300 2800 5300 2850
$EndSCHEMATC
