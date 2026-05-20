#include "system.h"
#include "drivers.h"
#include "protocol.h"
#include "crc.h"

// Globals from PLL
extern volatile uint8_t send_telem_flag;
extern volatile uint32_t telem_width;
extern volatile int32_t telem_error;
extern volatile uint32_t telem_output_errors;
extern volatile uint8_t telem_config_sequence;

// SysTick for Heartbeat
volatile uint32_t ms_ticks = 0;
void SysTick_Handler(void)
{
    ms_ticks++;
}
// TODO: Disable interrupts until init is done. 
int main(void)
{
    // Drivers Init
    RCC_Init();
    GPIO_Init();
    UART_Init();
    Timer_Init();
    CRC_Init();
    
    // Protocol Init
    Protocol_Init();
    
    // SysTick Init (1ms)
    // Load = SYSCLK_FREQ / 1000 - 1
    // STK_LOAD, STK_VAL, STK_CTRL
    *((volatile uint32_t*)0xE000E014) = (SYSCLK_FREQ / 1000) - 1; // LOAD
    *((volatile uint32_t*)0xE000E018) = 0;         // VAL
    *((volatile uint32_t*)0xE000E010) = 7;         // CTRL: Enable, Int, ClkSource(Processor)

    // ADC Init
    ADC_Init();

    uint32_t last_heartbeat = 0;
    uint32_t last_adc_sample = 0;

    while(1)
    {
        // 1. Process UART Logic 
        // We implemented ISR to echo? No, we need to read from DR if not using ISR ringbuffer.
        // Wait, UART ISR in `src/uart.c` reads and discards.
        // We should change UART ISR to feed Protocol.
        
        // 2. Check Telemetry
        Protocol_CheckTimeout();
        
        if (send_telem_flag)
        {
            send_telem_flag = 0;
#ifdef STM32G031xx
            uint8_t freq = 64;
#else
            uint8_t freq = 96;
#endif
            Protocol_SendTelemetry(telem_width, telem_error, telem_output_errors, telem_config_sequence, freq);
        }
        
        // 3. Sampling (1kHz)
        if (ms_ticks - last_adc_sample >= 1)
        {
           last_adc_sample = ms_ticks;
           ADC_Process();
        }

        // 4. Check Heartbeat (1Hz)
        if (ms_ticks - last_heartbeat >= 1000)
        {
            last_heartbeat = ms_ticks;
            
            uint8_t temp;
            uint16_t vcc_min, vcc_max, pb1_min, pb1_max;
            ADC_GetStats(&temp, &vcc_min, &vcc_max, &pb1_min, &pb1_max);
            
            Protocol_SendHeartbeat(temp, vcc_min, vcc_max, pb1_min, pb1_max);
        }
    }
}
