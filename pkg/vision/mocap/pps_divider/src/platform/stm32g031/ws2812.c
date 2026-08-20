#include "stm32g0xx.h"
#include "drivers.h"
#include <stdbool.h>

#define NUM_LEDS 2
#define RESET_PERIODS 240 // 240 PWM periods @ 1.25us = 300us
#define LED_DATA_PERIODS (NUM_LEDS * 24)
#define TOTAL_BUF_SIZE (RESET_PERIODS + LED_DATA_PERIODS + RESET_PERIODS + 1)

static uint16_t ws2812_buf[TOTAL_BUF_SIZE];
static uint32_t last_rgb_color = 0xFFFFFFFF;
static uint32_t last_led_update = 0;

void WS2812_Init(void) {
    // Enable TIM3 Clock
    RCC->APBENR1 |= RCC_APBENR1_TIM3EN;
    
    // Enable DMA1 Clock
    RCC->AHBENR |= RCC_AHBENR_DMA1EN;
    
    // Configure TIM3 for 800kHz PWM (64MHz / 80)
    TIM3->PSC = 0;
    TIM3->ARR = 79;
    
    // PWM Mode 1 on CH2 (OC2M = 110)
    TIM3->CCMR1 &= ~TIM_CCMR1_CC2S;
    TIM3->CCMR1 &= ~TIM_CCMR1_OC2M;
    TIM3->CCMR1 |= (6U << 12); // OC2M is bits 14:12
    TIM3->CCMR1 |= TIM_CCMR1_OC2PE;
    
    // Enable CH2 Output
    TIM3->CCER |= TIM_CCER_CC2E;
    
    // Enable DMA request on Update Event (UDE) - guarantees exactly 1 transfer per period
    TIM3->DIER |= TIM_DIER_UDE;
    
    // Initialize CCR2 to 0 (Idle state: 0% duty cycle -> MCU LOW -> MOSFET OFF -> 5V HIGH)
    TIM3->CCR2 = 0;
    
    // Enable TIM3
    TIM3->CR1 |= TIM_CR1_CEN;
    
    // Configure DMAMUX for DMA1_Channel1 -> TIM3_UP (Request 37 / 0x25)
    DMAMUX1_Channel0->CCR = 0x25;
    
    // Configure DMA1 Channel 1: Mem2Periph, 16-bit to 16-bit, MINC, DIR=Mem2Periph
    DMA1_Channel1->CCR = DMA_CCR_MINC | DMA_CCR_DIR | DMA_CCR_PSIZE_0 | DMA_CCR_MSIZE_0;
    
    // Pre-fill static portions of the DMA buffer
    for (int i = 0; i < RESET_PERIODS; i++) {
        ws2812_buf[i] = 80; // Pre-transfer reset
        ws2812_buf[RESET_PERIODS + LED_DATA_PERIODS + i] = 80; // Post-transfer reset
    }
    ws2812_buf[TOTAL_BUF_SIZE - 1] = 0; // Park pin
}

void WS2812_Update(uint32_t ms_ticks, uint32_t rgb_color) {
    // Check if an update is even needed
    if (rgb_color == last_rgb_color && (ms_ticks - last_led_update < 1000)) {
        return;
    }
    
    // If a transfer is currently in progress, exit early without blocking.
    // The condition above remains true, so we'll just try again on the next main loop iteration!
    if (DMA1_Channel1->CCR & DMA_CCR_EN) {
        if (!(DMA1->ISR & DMA_ISR_TCIF1)) {
            return; 
        }
        // Transfer complete! Clean up so we can start a new one.
        DMA1->IFCR = DMA_IFCR_CTCIF1;
        DMA1_Channel1->CCR &= ~DMA_CCR_EN;
    }
    
    last_rgb_color = rgb_color;
    last_led_update = ms_ticks;
    
    uint8_t r = (rgb_color >> 16) & 0xFF;
    uint8_t g = (rgb_color >> 8) & 0xFF;
    uint8_t b = (rgb_color >> 0) & 0xFF;
    
    // 2. LED Data
    uint32_t offset = RESET_PERIODS;
    uint32_t buf_idx = 0;
    for (int i = 0; i < NUM_LEDS; i++) {
        uint8_t colors[3] = {g, r, b};
        for (int c = 0; c < 3; c++) {
            uint8_t val = colors[c];
            for (int bit = 7; bit >= 0; bit--) {
                bool bit_val = (val & (1 << bit)) != 0;
                // Inverted Logic for 5V Level Shifter (1.25us period = 80 cycles):
                // WS2812 '0': 0.4us HIGH, 0.8us LOW -> MCU needs 0.4us LOW, 0.8us HIGH -> 0.8us / 1.25us * 80 = 51 cycles
                // WS2812 '1': 0.8us HIGH, 0.4us LOW -> MCU needs 0.8us LOW, 0.4us HIGH -> 0.4us / 1.25us * 80 = 26 cycles
                ws2812_buf[offset + buf_idx] = bit_val ? 26 : 51;
                buf_idx++;
            }
        }
    }
    
    // Transmit via DMA (non-blocking)
    DMA1_Channel1->CNDTR = TOTAL_BUF_SIZE;
    DMA1_Channel1->CMAR = (uint32_t)ws2812_buf;
    DMA1_Channel1->CPAR = (uint32_t)&TIM3->CCR2;
    DMA1->IFCR = DMA_IFCR_CTCIF1; // Clear any pending transfer complete flag
    DMA1_Channel1->CCR |= DMA_CCR_EN;
}
