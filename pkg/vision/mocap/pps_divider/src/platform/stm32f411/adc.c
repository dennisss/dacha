#include "stm32f4xx.h"
#include "drivers.h"

// The STM32F411 platform does not currently have a full ADC hardware implementation for this project.
// These stubs satisfy the linker for main.c and prevent hardfaults while providing safe default telemetry values.

void ADC_Init(void) {
    // Stub
}

void ADC_Process(void) {
    // Stub
}

void ADC_GetStats(uint8_t *temp_half_c, uint16_t *v_min, uint16_t *v_max, uint16_t *p_min, uint16_t *p_max) {
    // Provide safe defaults (25C, 3.3V, 0V)
    *temp_half_c = 50; // 50 * 0.5 = 25.0 C
    
    // 3.3V mapped to a 4095 scale is exactly 4095
    *v_min = 4095;
    *v_max = 4095;
    
    // PB1 default
    *p_min = 0;
    *p_max = 0;
}
