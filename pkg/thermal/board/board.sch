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
L Connector:Raspberry_Pi_2_3 J?
U 1 1 6150A25F
P 2600 2750
F 0 "J?" H 2600 4231 50  0000 C CNN
F 1 "Raspberry_Pi_2_3" H 2600 4140 50  0000 C CNN
F 2 "" H 2600 2750 50  0001 C CNN
F 3 "https://www.raspberrypi.org/documentation/hardware/raspberrypi/schematics/rpi_SCH_3bplus_1p0_reduced.pdf" H 2600 2750 50  0001 C CNN
	1    2600 2750
	1    0    0    -1  
$EndComp
$Comp
L Connector_Generic:Conn_01x10 J?
U 1 1 6150D5D4
P 1900 5350
F 0 "J?" H 1818 5967 50  0000 C CNN
F 1 "SparkFun TFT" H 1818 5876 50  0000 C CNN
F 2 "" H 1900 5350 50  0001 C CNN
F 3 "~" H 1900 5350 50  0001 C CNN
	1    1900 5350
	-1   0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 6150E54F
P 2350 4950
F 0 "#PWR?" H 2350 4700 50  0001 C CNN
F 1 "GND" V 2355 4822 50  0000 R CNN
F 2 "" H 2350 4950 50  0001 C CNN
F 3 "" H 2350 4950 50  0001 C CNN
	1    2350 4950
	0    -1   -1   0   
$EndComp
Wire Wire Line
	2350 4950 2100 4950
$EndSCHEMATC
