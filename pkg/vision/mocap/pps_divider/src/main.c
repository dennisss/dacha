#include "system.h"
#include "drivers.h"
#include "protocol.h"
#include "crc.h"
#include <stdbool.h>

// Globals from PLL
extern volatile uint8_t send_telem_flag;
extern volatile uint32_t telem_width;
extern volatile int32_t telem_error;
extern volatile uint32_t telem_output_errors;
extern volatile uint8_t telem_config_sequence;
extern volatile ConfigData_t pll_config;

// Globals for Setup and Build ID
volatile const uint64_t g_build_id = 0x0000000000000000;
volatile uint16_t g_pcb_revision = 0;
volatile bool g_setup_received = false;

// SysTick for Heartbeat
volatile uint32_t ms_ticks = 0;
void SysTick_Handler(void)
{
    ms_ticks++;
}

int main(void)
{
    // Drivers Init
    RCC_Init();
    GPIO_Init();
    UART_Init();
    Timer_Init();
    CRC_Init();
    
    // WS2812 Init
    WS2812_Init();
    
    // Protocol Init
    Protocol_Init();
    // SysTick Init (1ms)
    *((volatile uint32_t*)0xE000E014) = (SYSCLK_FREQ / 1000) - 1; // LOAD
    *((volatile uint32_t*)0xE000E018) = 0;         // VAL
    *((volatile uint32_t*)0xE000E010) = 7;         // CTRL: Enable, Int, ClkSource(Processor)

    // ADC Init
    ADC_Init();

    uint32_t last_heartbeat = 0;
    uint32_t last_adc_sample = 0;
    uint32_t last_led_update = 0;
    uint32_t last_rgb_color = 0xFFFFFFFF; // Force initial update

    while(1)
    {
        // 1. Process UART Logic 
        Protocol_CheckTimeout();
        
        // 2. Check Telemetry
        if (send_telem_flag)
        {
            send_telem_flag = 0;
            uint8_t freq = 64;
            Protocol_SendTelemetry(telem_width, telem_error, telem_output_errors, telem_config_sequence, freq);
        }
        
        // 3. WS2812 Update (on color change or every 1 sec)
        uint32_t current_color = pll_config.rgb_color;
        if (current_color != last_rgb_color || (ms_ticks - last_led_update >= 1000))
        {
            last_rgb_color = current_color;
            last_led_update = ms_ticks;
            WS2812_Update(current_color);
        }
        
        // 4. Sampling (1kHz)
        if (ms_ticks - last_adc_sample >= 1)
        {
           last_adc_sample = ms_ticks;
           ADC_Process();
        }

        // 5. Check Heartbeat (1Hz) - Gated by Setup
        if (g_setup_received && (ms_ticks - last_heartbeat >= 1000))
        {
            last_heartbeat = ms_ticks;
            
            uint8_t temp;
            uint16_t vcc_min, vcc_max, poe_min, poe_max;
            ADC_GetStats(&temp, &vcc_min, &vcc_max, &poe_min, &poe_max);
            
            Protocol_SendHeartbeat(temp, vcc_min, vcc_max, poe_min, poe_max);
        }
    }
}
