// Minimum PTP transmit timestamp that should definitely work on a Raspberry Pi 5 or CM5.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/ioctl.h>
#include <net/if.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <linux/net_tstamp.h>
#include <linux/sockios.h>
#include <linux/errqueue.h>
#include <errno.h>

#define IFACE_NAME "eth0"
#define PTP_PORT 319
#define PTP_GROUP "224.0.1.129"

// Exact flags from your strace: 69 = 0x45
// 1 (TX_HARDWARE) | 4 (RX_HARDWARE) | 64 (RAW_HARDWARE)
#define PTP4L_FLAGS (SOF_TIMESTAMPING_TX_HARDWARE | \
                     SOF_TIMESTAMPING_RX_HARDWARE | \
                     SOF_TIMESTAMPING_RAW_HARDWARE)

void die(const char *msg) {
    perror(msg);
    exit(EXIT_FAILURE);
}

void setup_hw_timestamping(int sock, const char *dev_name) {
    struct ifreq hwtstamp;
    struct hwtstamp_config hwconfig;

    memset(&hwtstamp, 0, sizeof(hwtstamp));
    memset(&hwconfig, 0, sizeof(hwconfig));
    strncpy(hwtstamp.ifr_name, dev_name, IFNAMSIZ - 1);
    hwtstamp.ifr_data = (void *)&hwconfig;

    // ptp4l enables both TX and RX hardware timestamping
    hwconfig.tx_type = HWTSTAMP_TX_ON;
    hwconfig.rx_filter = HWTSTAMP_FILTER_PTP_V2_EVENT;

    if (ioctl(sock, SIOCSHWTSTAMP, &hwtstamp) < 0) {
        // Fallback: Try "Filter All" if PTPV2 specific filter is rejected
        hwconfig.rx_filter = HWTSTAMP_FILTER_ALL;
        if (ioctl(sock, SIOCSHWTSTAMP, &hwtstamp) < 0) {
            die("ioctl(SIOCSHWTSTAMP)");
        }
    }
    printf("[INFO] PHY configured via ioctl\n");
}

int main(int argc, char *argv[]) {
    int sock;
    struct sockaddr_in addr;
    struct in_addr imr_interface;
    struct ifreq ifr;
    int flags = PTP4L_FLAGS;
    int select_err = 1;
    
    // 1. Create UDP Socket (Matches strace: socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP))
    sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock < 0) die("socket");

    // 2. Bind to Device (Matches strace: setsockopt(SO_BINDTODEVICE))
    if (setsockopt(sock, SOL_SOCKET, SO_BINDTODEVICE, IFACE_NAME, strlen(IFACE_NAME)) < 0) {
        die("setsockopt(SO_BINDTODEVICE)");
    }

    // 3. Set SO_TIMESTAMPING to 69 (Matches strace)
    if (setsockopt(sock, SOL_SOCKET, SO_TIMESTAMPING, &flags, sizeof(flags)) < 0) {
        die("setsockopt(SO_TIMESTAMPING)");
    }

    // 4. Set SO_SELECT_ERR_QUEUE (Matches strace)
    // This allows select() to return readable when data is in the error queue
    if (setsockopt(sock, SOL_SOCKET, SO_SELECT_ERR_QUEUE, &select_err, sizeof(select_err)) < 0) {
        perror("setsockopt(SO_SELECT_ERR_QUEUE) - continuing anyway");
    }

    // 5. Configure PHY Driver
    setup_hw_timestamping(sock, IFACE_NAME);

    // 6. Set Multicast Interface (Matches strace IP_MULTICAST_IF)
    // We get the IP of eth0 first
    strncpy(ifr.ifr_name, IFACE_NAME, IFNAMSIZ-1);
    if (ioctl(sock, SIOCGIFADDR, &ifr) < 0) {
        die("ioctl(SIOCGIFADDR) - Ensure eth0 has an IP address assigned!");
    }
    imr_interface = ((struct sockaddr_in *)&ifr.ifr_addr)->sin_addr;

    if (setsockopt(sock, SOL_IP, IP_MULTICAST_IF, &imr_interface, sizeof(struct in_addr)) < 0) {
        die("setsockopt(IP_MULTICAST_IF)");
    }

    // 7. Construct PTP Sync Packet (44 bytes)
    // Matches strace payload: \0\2\0,\0\0\2\0...
    uint8_t payload[44];
    memset(payload, 0, 44);
    
    payload[0] = 0x00; // MsgType: Sync (0)
    payload[1] = 0x02; // Version: 2
    payload[2] = 0x00; // Length High
    payload[3] = 0x2c; // Length Low (44 bytes - matches ASCII ',')
    payload[4] = 0x00; // Domain 0
    payload[5] = 0x00; // Reserved
    
    // CRITICAL: FLAGS = 0x0200 (Two-Step Flag)
    // Trace byte 6 is \2 (0x02). This tells HW "Don't modify packet, just record time".
    payload[6] = 0x02; 
    payload[7] = 0x00; 

    // Sequence ID
    payload[30] = 0x00;
    payload[31] = 0x01;

    // 8. Send to 224.0.1.129:319
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons(PTP_PORT);
    addr.sin_addr.s_addr = inet_addr(PTP_GROUP);

    printf("[INFO] Sending PTP Sync (UDP) to %s:%d with Two-Step Flag...\n", PTP_GROUP, PTP_PORT);
    if (sendto(sock, payload, sizeof(payload), 0, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        die("sendto");
    }

    // 9. Poll Error Queue
    fd_set errorfs;
    struct timeval timeout = { .tv_sec = 2, .tv_usec = 0 };
    FD_ZERO(&errorfs);
    FD_SET(sock, &errorfs);

    // Because we set SO_SELECT_ERR_QUEUE, the error queue might signal as 'readable'
    // but usually, it signals as an exceptional condition or readable depending on kernel version.
    // We check both to be safe.
    fd_set readfs;
    FD_ZERO(&readfs);
    FD_SET(sock, &readfs);

    int res = select(sock + 1, &readfs, NULL, &errorfs, &timeout);

    if (res > 0) {
        char data[512];
        struct msghdr msg;
        struct iovec iov;
        char control[512];
        
        memset(&msg, 0, sizeof(msg));
        iov.iov_base = data;
        iov.iov_len = sizeof(data);
        msg.msg_iov = &iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control;
        msg.msg_controllen = sizeof(control);

        // Try reading from ERRQUEUE
        int n = recvmsg(sock, &msg, MSG_ERRQUEUE);
        if (n < 0) {
             // If EAGAIN, maybe it wasn't the errqueue that woke us up
             perror("recvmsg failed");
        } else {
            struct cmsghdr *cmsg;
            for (cmsg = CMSG_FIRSTHDR(&msg); cmsg; cmsg = CMSG_NXTHDR(&msg, cmsg)) {
                if (cmsg->cmsg_level == SOL_SOCKET && cmsg->cmsg_type == SO_TIMESTAMPING) {
                    struct scm_timestamping *ts = (struct scm_timestamping *)CMSG_DATA(cmsg);
                    
                    printf("\n[SUCCESS] GOLDEN TIMESTAMP RETRIEVED!\n");
                    printf("  System Time: %lu.%09lu\n", ts->ts[0].tv_sec, ts->ts[0].tv_nsec);
                    printf("  PTP HW Time: %lu.%09lu <--- FROM PHY\n", ts->ts[2].tv_sec, ts->ts[2].tv_nsec);
                    return 0;
                }
            }
            printf("[WARN] Packet in Error Queue, but no Timestamp CMSG found.\n");
        }
    } else {
        printf("[FAIL] Timeout. PHY ignored the packet.\n");
    }

    close(sock);
    return 0;
}