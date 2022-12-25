
Helpful guide: https://forums.serverbuilds.net/t/official-aruba-s2500-managed-ethernet-switch-poe-10gsfp/5038

First reset to factory:
- Hit "Menu" button until on "Maintainence" and then hit "Enter"
- Then go to the "Factory Default" and press "Enter" on it.


While it is rebooting, smtart monitoring over RJ45 serial console:

```
stty -F /dev/ttyUSB0 cs8 -parenb -cstopb -crtscts

screen /dev/ttyUSB0 9600
```

During first foot it prints out:

```
xls_gmac0 [PRIME], xls_gmac1, xls_gmac2, xls_gmac3
Boot:  Primary bootflash partition
Reboot code: 0:2b: 5:38:24 

Hit any key to stop autoboot:  0 
booting system partition 0:0
USB:   USB 2.0 started, EHCI 1.00scanning bus for devices... 2 USB Device(s) found
       scanning bus for storage devices... max USB Storage Device reached: 1 stopping
1 Storage Device(s) found
1 blocks read: OK
Loading image 0:0............................................................................................................................................................................................................
Booting image...
Signer Cert OK
Policy Cert OK
RSA signature verified. 

Aruba Networks
ArubaOS Version 7.4.1.12 (build 72393 / label #72393) 
Built by p4build@corfu on 2019-09-24 at 00:42:27 PDT (gcc version 3.4.3)
Copyright (c) 2016 Aruba, a Hewlett Packard Enterprise company.

        <<<<<    Welcome to Aruba Networks - Aruba S2500-48P-US    >>>>>

Performing CompactFlash fast test...  Checking for file system...
Passed.
Reboot Cause: User reboot (0x86:0x78:0x402b)
Initializing TPM and Certificates
TPM and Certificate Initialization successful.
Loading factory initial configuration.

******************************************** 
Starting Console Session 
Retrieving Configuration ... 
******************************************** 



(ArubaS2500-48P-US) 
User: 
```

Enter `admin` as the user name and `admin123` as the password.

Doesn't seem to be use-able.

Next user "GUI Setup Mode"

Connect computer to a single front ethernet port and use go to "172.16.0.254".


Try to ssh with `ssh admin@10.1.0.104`, but this gets an error:

```
Unable to negotiate with 10.1.0.104 port 22: no matching cipher found. Their offer: aes128-cbc,aes256-cbc
```

So we need to force it with `ssh -oCiphers=+aes256-cbc admin@10.1.0.104`

Enter priveleged mode with the `en` command.

Then use the following commands to enable all 4 SFP+ ports:

```
delete stacking interface stack 1/2
delete stacking interface stack 1/3
```

Initially set password to `password`.


Printing SFP cable information:

```
show interface gigabitethernet 0/1/0 transceiver detail
```

I get the following output for a 10GTek DAC cable:

```
Vendor Name                                : OEM            
Vendor Serial Number                       : CSC210601530138
Vendor Part Number                         : SFP-H10GB-CU1M
Aruba Certified                            : NO
Cable Type                                 : unknown
Connector Type                             : Copper Pigtail
Wave Length                                : 256 nm
Cable Length                               : 1m
```
