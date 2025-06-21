

Bridge:

- LAN:
    - Receive IP packets
        - From: `[Internal IP]:[Internal TCP/UDP Port]`
            - Rename to `[External IP]:[External TCP/UDP Port]`
        - To:  `[External IP]:[External TCP/UDP Port]`
        - Also need to support ICMP packets
    - May also receive packets to the router

- WAN: DHCP to get an IP address
    - Gemini
        The ISP's DHCP server responds (via the modem) and assigns a public IP address, subnet mask, default gateway address (usually the ISP's next-hop router), and DNS server addresses to your router's WAN (Wide Area Network) port. This is the information your router needs to communicate with the internet.

- WiFi
    - `hostapd`
    - https://w1.fi/wpa_supplicant/devel/hostapd_ctrl_iface_page.html

Router needs to maintain
    - Public IP Address
    - Internal UDP/TCP port + IP to external UDP/TCP port mapping
    - DHCP Server for internal IPs
    - DHCP Client for external IP

Challenge is how to make the router work like a cluster node?
- Fundamentally we have on interface with 2 IP addresses ()


- How to handle this:
    - Raw socket https://man7.org/linux/man-pages/man7/raw.7.html
        - Also need an IPv6 socket
        - `socket(AF_INET, SOCK_RAW, IPPROTO_IP)`
        - `recvmsg` to all messages
    - 'SO_BINDTODEVICE' to bind to a single device
    - Should disable ICMP response in the kernel https://askubuntu.com/questions/1402550/to-disable-and-remove-the-ping-service


Milestones
- Standalone wifi + 2 port ethernet switch
- Add DHCP server so that it is a router