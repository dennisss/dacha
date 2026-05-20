#include "crc.h"


// Standard STM32 CRC-32 (IEEE 802.3)
// Polynomial: 0x04C11DB7

#define CRC32_POLY 0x04C11DB7

void CRC_Init(void)
{
    // No initialization needed for software implementation
}

static uint32_t CRC_Update(uint32_t crc, uint8_t data)
{
    crc ^= ((uint32_t)data << 24);
    for (int i = 0; i < 8; i++) {
        if (crc & 0x80000000) {
            crc = (crc << 1) ^ CRC32_POLY;
        } else {
            crc <<= 1;
        }
    }
    return crc;
}

uint32_t CRC_Calculate(const uint8_t *data, uint32_t len, uint32_t current_crc)
{
    uint32_t crc = current_crc;
    for (uint32_t i = 0; i < len; i++) {
        crc = CRC_Update(crc, data[i]);
    }
    return crc;
}
