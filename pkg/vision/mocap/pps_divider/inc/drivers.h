#ifndef DRIVERS_H
#define DRIVERS_H

#include <stdint.h>

void RCC_Init(void);
void GPIO_Init(void);
void UART_Init(void);
void Timer_Init(void);

// UART Util
void UART_Write(uint8_t *data, uint32_t len);
void UART_WriteByte(uint8_t byte);

// ADC
void ADC_Init(void);
void ADC_Process(void);
void ADC_GetStats(uint8_t *temp_half_c, uint16_t *v_min, uint16_t *v_max, uint16_t *p_min, uint16_t *p_max);

// WS2812
void WS2812_Init(void);
void WS2812_Update(uint32_t ms_ticks, uint32_t rgb_color);

#endif
