# Optical Motion Capture : Ethernet Adapter

In order to use the mocap cameras, you will need to directly connect the network switch to your host computer via an ethernet cable. DOT NOT connect the cameras to your home network directly.

So, if your computer already has an unused ethernet port, use that and stop reading this page, else, you will need to buy an adapter and plug it into your computer.

## Recommendations

Depending on what computer you have, you will likely be able to support different protocols. Below is the summary of the best to worst types of network adapters to get. For just doing tracking, you should be ok with any 1Gbps option. If you want to do video streaming, I'd recommend upgrading to 10Gbps (this mainly matters for the connection between your network switch and your host computer, but not all network switches support this).

**(Best)** If you have a desktop computer, use a PCIe to ethernet card:

- 1Gbps: https://www.amazon.com/TP-Link-1000Mbps-Gigabit-Ethernet-supported/dp/B003CFATNI
- 10Gbps: https://www.amazon.com/TP-Link-TX401-Ethernet-Supports-Including/dp/B08D71PVXG
- 10Gbps (SFP+) : https://www.amazon.com/10Gtek-E10G42BTDA-Ethernet-Converged-X520-DA2/dp/B01DCZCA3O
    - If you have a network switch with an SFP port, just get a 'direct attach copper' SFP+ cable to go between them.

**(Great)**: If you have USB4 or Thunderbolt (e.g. macOS), get an ethernet adapter that supports one of these.

- Apple's old Thunberbolt to Ethernet adapter was great if your computer supports it, but not sure if there are newer cheap options...
- Note: A LOT of cheap ethernet adapters say they 'support' these but internally that just means they use one of the older protocols listed below and are forwards compatible.

**(Good)**: Use a USB 3.2 Gen 2 or better USB adapter if your computer supports this USB version

- 5Gbps: https://www.amazon.com/Cable-Matters-Gigabit-Ethernet-Adapter/dp/B0D3FM7Z4L
- 10Gbps: https://www.amazon.com/Cable-Matters-Compatible-Thunderbolt-Real-World/dp/B0G1WJFR5V
- These are only worth the upgrade over the `Ok` options if you want to do a lot of video streaming.

**(Ok)**: Use a USB 3.0 or better to ethernet adapter:

- 1Gbps https://www.amazon.com/dp/B08CK9X9Z8
- Not recommended for continuous video streaming.

Do not use USB 2.0 or worse.
