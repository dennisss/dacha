# EdgeSwitch Notes

Below is information on the ES-8-150W ethernet switch.

- Hardware
    - Processor: BCM53343A0KFSBLG
    - Flash ROM: MX25L25645GMI
        - 256Mb (32MB)
    - RAM Chip: UniIC SCB15H2G160
        - DDR3 SDRAM
        - 2Gb (256MB)
    - Ethernet Transformer
        - M-Tek G48209SNG
- Software
    - Linux OS
    - HTTP Frontend
        - lighttpd: Used for static file serving and forwards to an internal API binary.
        - Default Certificate: PKCS #1 SHA-256 With RSA Encryption
        - TLS 1.2, ECDHE_RSA with P-384, and AES_128_GCM.


## SSH

49
53
39
39


```
(UBNT EdgeSwitch) >show environment

Temperature Sensors:
Unit     Sensor  Description       Temp (C)    State           Max_Temp (C)
----     ------  ----------------  ----------  --------------  --------------
1        1       TEMP-1            48          Normal          48
1        2       TEMP-2            53          Normal          53
1        3       PoE-01            39          Normal          39
1        4       PoE-02            39          Normal          39
```



## HTTP API


### Login

Note: default username and password is `ubnt`.

Login with wrong user/pass:

```
POST https://10.1.0.49/api/v1.0/user/login
REQUEST: {"username":"ubnt","password":"asas"}
HTTP STATUS: 401
RESPONSE: {"statusCode":401,"error":1,"detail":"User account invalid (does not exist or invalid password).","message":"Failure"}
```

Login with right user/pass:

```
Returns "x-auth-token" header in the response
Response Body: {"statusCode":200,"error":0,"detail":"User account valid.","message":"Success"}
```

### Services


```
GET https://10.1.0.49/api/v1.0/services
Request Header: x-auth-token
```

```
{"discoveryResponder":{"enabled":true},"discoveryScanner":{"enabled":true,"passiveOnly":true},"sshServer":{"enabled":true,"sshPort":22},"telnetServer":{"enabled":false,"port":23},"webServer":{"enabled":true,"httpPort":80,"httpsPort":443},"systemLog":{"enabled":false,"port":null,"server":null,"level":"emerg"},"ntpClient":{"enabled":true,"ntpServers":["1.ubnt.pool.ntp.org","2.ubnt.pool.ntp.org"]},"unms":{"enabled":false,"key":"","status":null},"lldp":{"enabled":true}}
```

### Device

```
GET https://10.1.0.49/api/v1.0/device
```

Response:

```
{"errorCodes":[],"identification":{"mac":"68:d7:9a:67:00:55","model":"ES-8-150W","family":"EdgeSwitch","firmwareVersion":"1.9.2","firmware":"ES.bcmwh.v1.9.2.5322630.200807.0830","product":"EdgeSwitch 8 150W","serverVersion":"1.1.3","bridgeVersion":"0.12.2"},"capabilities":{"interfaces":[{"id":"0\/1","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/2","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/3","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/4","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/5","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/6","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/7","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/8","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":true,"supportCableTest":true,"poeValues":["off","active","24v"],"media":"GE","speedValues":["10-full","10-half","100-full","100-half","1000-full","auto"]},{"id":"0\/9","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":false,"supportCableTest":false,"media":"SFP","speedValues":["1000-full","auto","autodetect"]},{"id":"0\/10","type":"port","supportBlock":true,"supportDelete":false,"supportReset":true,"configurable":true,"supportDHCPSnooping":true,"supportIsolate":true,"supportAutoEdge":true,"maxMTU":9216,"supportPOE":false,"supportCableTest":false,"media":"SFP","speedValues":["1000-full","auto","autodetect"]},{"id":"3\/1","type":"lag","supportBlock":true,"supportDelete":true,"supportReset":false,"configurable":true,"supportLinkTrap":true,"loadBalanceValues":["src_mac_l2","dst_mac_l2","src_dst_mac_l2","src_ip_port","dst_ip_port","src_dst_ip_port"],"maxMTU":9216},{"id":"3\/2","type":"lag","supportBlock":true,"supportDelete":true,"supportReset":false,"configurable":true,"supportLinkTrap":true,"loadBalanceValues":["src_mac_l2","dst_mac_l2","src_dst_mac_l2","src_ip_port","dst_ip_port","src_dst_ip_port"],"maxMTU":9216},{"id":"3\/3","type":"lag","supportBlock":true,"supportDelete":true,"supportReset":false,"configurable":true,"supportLinkTrap":true,"loadBalanceValues":["src_mac_l2","dst_mac_l2","src_dst_mac_l2","src_ip_port","dst_ip_port","src_dst_ip_port"],"maxMTU":9216},{"id":"3\/4","type":"lag","supportBlock":true,"supportDelete":true,"supportReset":false,"configurable":true,"supportLinkTrap":true,"loadBalanceValues":["src_mac_l2","dst_mac_l2","src_dst_mac_l2","src_ip_port","dst_ip_port","src_dst_ip_port"],"maxMTU":9216},{"id":"3\/5","type":"lag","supportBlock":true,"supportDelete":true,"supportReset":false,"configurable":true,"supportLinkTrap":true,"loadBalanceValues":["src_mac_l2","dst_mac_l2","src_dst_mac_l2","src_ip_port","dst_ip_port","src_dst_ip_port"],"maxMTU":9216},{"id":"3\/6","type":"lag","supportBlock":true,"supportDelete":true,"supportReset":false,"configurable":true,"supportLinkTrap":true,"loadBalanceValues":["src_mac_l2","dst_mac_l2","src_dst_mac_l2","src_ip_port","dst_ip_port","src_dst_ip_port"],"maxMTU":9216}],"services":["TELNET_SERVER","SSH_SERVER","DISCOVERY_RESPONDER","UNMS","NTP","LLDP","WEB_SERVER","SYSTEM_LOG"],"device":{"supportDeviceBackup":true,"supportFirmwareUpgrade":true,"supportManagementConfig":true,"supportManagementVLAN":true,"hasGlobalMTU":false,"hasGlobalLoadBalance":false,"supportFallbackConfig":false,"supportLedsOff":false,"supportPositioning":false,"supportAnalytics":true,"supportCrashReporting":true,"defaultFallbackAddress":"192.168.1.2\/24","supportedUdapiVersion":["1.0"]},"tools":["PING","TRACEROUTE","MAC_TABLE","CABLE_TEST","DISCOVERY_SCANNER","SPEEDTEST","TUNNEL"],"vlanSwitching":{"supported":true,"supportsQinQ":false,"supportsRanges":false,"supportsTrunkUndefinedVLANs":false,"defaultVLAN":1,"maxID":4093},"uas":false,"wifi":{"supported":false}}}
```

### System

```
GET https://10.1.0.49/api/v1.0/system
```

Response:

```
{"hostname":"UBNT EdgeSwitch","timezone":"Other","domainName":"","factoryDefault":false,"stp":{"enabled":true,"version":"MSTP","maxAge":20,"helloTime":2,"forwardDelay":15,"priority":32768},"analyticsEnabled":false,"dnsServers":[{"type":"dynamic","version":"v4","address":"10.1.0.1","origin":"dhcp"}],"defaultGateway":[{"type":"dynamic","version":"v4","address":"10.1.0.1","origin":"dhcp"}],"users":[{"username":"ubnt","readOnly":false}],"management":{"vlanID":1,"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}]}}
```

### Statistics

```
GET https://10.1.0.49/api/v1.0/statistics
```

Response:

```
[{"timestamp":1628436689057,"device":{"cpu":[{"identifier":"ARMv7 Processor rev 1 (v7l)","usage":72}],"ram":{"usage":73,"free":69201920,"total":262553600},"temperatures":[{"name":"TEMP-1","type":"other","value":67.000000},{"name":"TEMP-2","type":"other","value":71.000000},{"name":"PoE-01","type":"other","value":53.000000},{"name":"PoE-02","type":"other","value":53.000000}],"power":[],"storage":[],"fanSpeeds":[],"uptime":1306},"interfaces":[{"id":"0\/1","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/2","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/3","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/4","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/5","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/6","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/7","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/8","name":"","statistics":{"dropped":0,"errors":934,"txErrors":0,"rxErrors":934,"rate":0,"txRate":0,"rxRate":0,"bytes":13204,"txBytes":13204,"rxBytes":0,"packets":45,"txPackets":45,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"0\/9","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":57064,"txRate":44080,"rxRate":12984,"bytes":10381492,"txBytes":9415375,"rxBytes":966117,"packets":14776,"txPackets":8115,"rxPackets":6661,"pps":14,"txPPS":7,"rxPPS":7,"sfp":{"temperature":null,"voltage":null,"current":null,"rxPower":null,"txPower":null}}},{"id":"0\/10","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"sfp":{"temperature":null,"voltage":null,"current":null,"rxPower":null,"txPower":null}}},{"id":"3\/1","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"3\/2","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"3\/3","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"3\/4","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"3\/5","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}},{"id":"3\/6","name":"","statistics":{"dropped":0,"errors":0,"txErrors":0,"rxErrors":0,"rate":0,"txRate":0,"rxRate":0,"bytes":0,"txBytes":0,"rxBytes":0,"packets":0,"txPackets":0,"rxPackets":0,"pps":0,"txPPS":0,"rxPPS":0,"poePower":0.000000}}]}]
```

### Interfaces

```
GET https://10.1.0.49/api/v1.0/interfaces
```

Response:

```
[{"identification":{"id":"0\/1","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688538,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/2","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688548,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/3","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688585,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/4","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688591,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/5","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688597,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/6","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688635,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/7","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688672,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"active","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/8","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688678,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"off","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}},{"identification":{"id":"0\/9","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688772,"enabled":true,"comment":"","description":"1 Gbps - Full Duplex","plugged":true,"currentSpeed":"1000-full","speed":"autodetect","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"sfp":{"present":true,"vendor":"OEM","part":"SFP-GE-T","serial":"CSMTGTL102174","txFault":null,"los":null},"poe":"off","flowControl":false,"routed":false,"isolated":false}},{"identification":{"id":"0\/10","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436688782,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"autodetect","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49\/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55\/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"sfp":{"present":false,"vendor":"","part":"","serial":"","txFault":null,"los":null},"poe":"off","flowControl":false,"routed":false,"isolated":false}},{"identification":{"id":"3\/1","name":"","mac":"68:d7:9a:67:00:55","type":"lag"},"status":{"timestamp":1628436688791,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"","arpProxy":false,"mtu":1518,"cableLength":0},"addresses":[],"lag":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":96},"dhcpSnooping":false,"static":false,"linkTrap":true,"loadBalance":"src_dst_mac_l2","interfaces":[]}},{"identification":{"id":"3\/2","name":"","mac":"68:d7:9a:67:00:55","type":"lag"},"status":{"timestamp":1628436688795,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"","arpProxy":false,"mtu":1518,"cableLength":0},"addresses":[],"lag":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":96},"dhcpSnooping":false,"static":false,"linkTrap":true,"loadBalance":"src_dst_mac_l2","interfaces":[]}},{"identification":{"id":"3\/3","name":"","mac":"68:d7:9a:67:00:55","type":"lag"},"status":{"timestamp":1628436688798,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"","arpProxy":false,"mtu":1518,"cableLength":0},"addresses":[],"lag":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":96},"dhcpSnooping":false,"static":false,"linkTrap":true,"loadBalance":"src_dst_mac_l2","interfaces":[]}},{"identification":{"id":"3\/4","name":"","mac":"68:d7:9a:67:00:55","type":"lag"},"status":{"timestamp":1628436688803,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"","arpProxy":false,"mtu":1518,"cableLength":0},"addresses":[],"lag":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":96},"dhcpSnooping":false,"static":false,"linkTrap":true,"loadBalance":"src_dst_mac_l2","interfaces":[]}},{"identification":{"id":"3\/5","name":"","mac":"68:d7:9a:67:00:55","type":"lag"},"status":{"timestamp":1628436688807,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"","arpProxy":false,"mtu":1518,"cableLength":0},"addresses":[],"lag":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":96},"dhcpSnooping":false,"static":false,"linkTrap":true,"loadBalance":"src_dst_mac_l2","interfaces":[]}},{"identification":{"id":"3\/6","name":"","mac":"68:d7:9a:67:00:55","type":"lag"},"status":{"timestamp":1628436688811,"enabled":true,"comment":"","description":"","plugged":false,"currentSpeed":"","speed":"","arpProxy":false,"mtu":1518,"cableLength":0},"addresses":[],"lag":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":96},"dhcpSnooping":false,"static":false,"linkTrap":true,"loadBalance":"src_dst_mac_l2","interfaces":[]}}]
```

Can also set an interface. Example of turning off POE on one port:

```
PUT https://10.1.0.49/api/v1.0/interfaces

Request Body: [{"identification":{"id":"0/8","name":"","mac":"68:d7:9a:67:00:55","type":"port"},"status":{"timestamp":1628436649882,"enabled":true,"comment":"","description":"1 Gbps - Full Duplex","plugged":true,"currentSpeed":"1000-full","speed":"auto","arpProxy":true,"mtu":1518,"cableLength":0},"addresses":[{"type":"dynamic","version":"v4","cidr":"10.1.0.49/16","eui64":false,"origin":"dhcp"},{"type":"dynamic","version":"v6","cidr":"fe80::6ad7:9aff:fe67:55/64","eui64":true,"origin":"linkLocal"}],"port":{"stp":{"enabled":true,"edgePort":"auto","pathCost":0,"portPriority":128},"dhcpSnooping":false,"poe":"off","flowControl":false,"routed":false,"isolated":false,"pingWatchdog":{"enabled":false,"address":"0.0.0.0","failureCount":3,"interval":15,"offDelay":5,"startDelay":300}}}]
```

