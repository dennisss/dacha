#include "stm32g0xx.h"
#include "drivers.h"
#include <stdbool.h>

#define NUM_LEDS 12
static uint8_t ws2812_buf[NUM_LEDS * 12];

void WS2812_Init(void) {
    // SPI1 at 4MHz (64MHz / 16) -> BR = 011
    SPI1->CR1 = SPI_CR1_MSTR | (3 << SPI_CR1_BR_Pos) | SPI_CR1_SSI | SPI_CR1_SSM;
    SPI1->CR2 = (7 << SPI_CR2_DS_Pos) | SPI_CR2_FRXTH; 
    SPI1->CR1 |= SPI_CR1_SPE;
}

void WS2812_Update(uint32_t rgb_color) {
    uint8_t r = rgb_color & 0xFF;
    uint8_t g = (rgb_color >> 8) & 0xFF;
    uint8_t b = (rgb_color >> 16) & 0xFF;
    
    uint32_t buf_idx = 0;
    for (int i = 0; i < NUM_LEDS; i++) {
        uint8_t colors[3] = {g, r, b};
        for (int c = 0; c < 3; c++) {
            uint8_t val = colors[c];
            for (int bit = 7; bit >= 0; bit--) {
                bool bit_val = (val & (1 << bit)) != 0;
                // Original: bit_val ? 0b1110 : 0b1000
                // Inverted for level shifter: bit_val ? 0b0001 : 0b0111
                uint8_t spi_nibble = bit_val ? 0b0001 : 0b0111;
                
                if ((buf_idx % 2) == 0) {
                    ws2812_buf[buf_idx / 2] = (spi_nibble << 4);
                } else {
                    ws2812_buf[buf_idx / 2] |= spi_nibble;
                }
                buf_idx++;
            }
        }
    }
    
    for (uint32_t i = 0; i < sizeof(ws2812_buf); i++) {
        while (!(SPI1->SR & SPI_SR_TXE));
        *((volatile uint8_t *)&SPI1->DR) = ws2812_buf[i];
    }
    while (SPI1->SR & SPI_SR_BSY);
}
