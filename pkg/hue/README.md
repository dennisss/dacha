
Related past work:
- Official documentation:
    - https://developers.meethue.com/develop/hue-api/lights-api/#get-all-lights
    - https://developers.meethue.com/develop/hue-api-v2/getting-started/
    - Simplest way to find a hue is to go to https://discovery.meethue.com/
    - Recommended local discovery mode is mdns:
        - https://developers.meethue.com/develop/application-design-guidance/hue-bridge-discovery/
- http://www.burgestrand.se/hue-api/api/discovery/#udp-broadcast
- https://github.com/tigoe/hue-control


```
$ dig -p 5353 @224.0.0.251 _hue._tcp.local PTR

; <<>> DiG 9.16.1-Ubuntu <<>> -p 5353 @224.0.0.251 _hue._tcp.local PTR
; (1 server found)
;; global options: +cmd
;; Got answer:
;; WARNING: .local is reserved for Multicast DNS
;; You are currently testing what happens when an mDNS query is leaked to DNS
;; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 60331
;; flags: qr aa; QUERY: 1, ANSWER: 1, AUTHORITY: 0, ADDITIONAL: 4

;; QUESTION SECTION:
;_hue._tcp.local.		IN	PTR

;; ANSWER SECTION:
_hue._tcp.local.	10	IN	PTR	Philips\032Hue\032-\0327BF9F0._hue._tcp.local.

;; ADDITIONAL SECTION:
Philips\032Hue\032-\0327BF9F0._hue._tcp.local. 10 IN SRV 0 0 443 0017887bf9f0.local.
Philips\032Hue\032-\0327BF9F0._hue._tcp.local. 10 IN TXT "bridgeid=001788fffe7bf9f0" "modelid=BSB002"
0017887bf9f0.local.	10	IN	A	10.1.0.52
0017887bf9f0.local.	10	IN	AAAA	fe80::217:88ff:fe7b:f9f0

;; Query time: 0 msec
;; SERVER: 10.1.0.52#5353(224.0.0.251)
;; WHEN: Wed Feb 23 20:10:02 PST 2022
;; MSG SIZE  rcvd: 198
```
