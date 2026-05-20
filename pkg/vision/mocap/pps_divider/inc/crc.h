#ifndef CRC_H
#define CRC_H

#include <stdint.h>

#define CRC_INITIAL_VALUE 0xFFFFFFFF

/**
 * @brief Initialize CRC hardware (if applicable) or software state.
 */
void CRC_Init(void);

/**
 * @brief Calculate CRC over a buffer.
 * 
 * @param data Pointer to data buffer
 * @param len Length of data in bytes
 * @param current_crc Starting CRC value (use CRC_INITIAL_VALUE for new calculation)
 * @return Final CRC value
 */
uint32_t CRC_Calculate(const uint8_t *data, uint32_t len, uint32_t current_crc);

#endif
