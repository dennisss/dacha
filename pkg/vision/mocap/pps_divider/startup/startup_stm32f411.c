#include <stdint.h>

#define SRAM_START 0x20000000U
#define SRAM_SIZE  (128U * 1024U) // 128KB
#define SRAM_END   ((SRAM_START) + (SRAM_SIZE))

#define STACK_START SRAM_END

extern uint32_t _etext;
extern uint32_t _sdata;
extern uint32_t _edata;
extern uint32_t _sbss;
extern uint32_t _ebss;
extern uint32_t _sidata;

int main(void);
void SystemInit(void); // Defined in system initialization if needed, or dummy

void Reset_Handler(void);
void Default_Handler(void);

// Weak aliases for all interrupts
void NMI_Handler(void)          __attribute__((weak, alias("Default_Handler")));
void HardFault_Handler(void)    __attribute__((weak, alias("Default_Handler")));
void MemManage_Handler(void)    __attribute__((weak, alias("Default_Handler")));
void BusFault_Handler(void)     __attribute__((weak, alias("Default_Handler")));
void UsageFault_Handler(void)   __attribute__((weak, alias("Default_Handler")));
void SVC_Handler(void)          __attribute__((weak, alias("Default_Handler")));
void DebugMon_Handler(void)     __attribute__((weak, alias("Default_Handler")));
void PendSV_Handler(void)       __attribute__((weak, alias("Default_Handler")));
void SysTick_Handler(void)      __attribute__((weak, alias("Default_Handler")));

// External Interrupts (subset relevant to F411)
void EXTI0_IRQHandler(void)     __attribute__((weak, alias("Default_Handler")));
void EXTI1_IRQHandler(void)     __attribute__((weak, alias("Default_Handler")));
void EXTI2_IRQHandler(void)     __attribute__((weak, alias("Default_Handler")));
void TIM2_IRQHandler(void)      __attribute__((weak, alias("Default_Handler")));
void USART1_IRQHandler(void)    __attribute__((weak, alias("Default_Handler")));

// Placeholder definitions for IRQ handlers to ensure the vector table compiles correctly.
void TIM1_BRK_TIM9_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TIM1_UP_TIM10_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TIM1_TRG_COM_TIM11_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TIM1_CC_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));


uint32_t * g_pfnVectors[] __attribute__((section(".isr_vector"))) = {
    (uint32_t *)STACK_START,
    (uint32_t *)Reset_Handler,
    (uint32_t *)NMI_Handler,
    (uint32_t *)HardFault_Handler,
    (uint32_t *)MemManage_Handler,
    (uint32_t *)BusFault_Handler,
    (uint32_t *)UsageFault_Handler,
    0,
    0,
    0,
    0,
    (uint32_t *)SVC_Handler,
    (uint32_t *)DebugMon_Handler,
    0,
    (uint32_t *)PendSV_Handler,
    (uint32_t *)SysTick_Handler,
    
    // External Interrupts
    0,                  // WWDG
    0,                  // PVD
    0,                  // TAMP_STAMP
    0,                  // RTC_WKUP
    0,                  // FLASH
    0,                  // RCC
    (uint32_t *)EXTI0_IRQHandler, // EXTI0
    (uint32_t *)EXTI1_IRQHandler, // EXTI1
    (uint32_t *)EXTI2_IRQHandler, // EXTI2
    0,                  // EXTI3
    0,                  // EXTI4
    0,                  // DMA1_Stream0
    0,                  // DMA1_Stream1
    0,                  // DMA1_Stream2
    0,                  // DMA1_Stream3
    0,                  // DMA1_Stream4
    0,                  // DMA1_Stream5
    0,                  // DMA1_Stream6
    0,                  // ADC
    0,                  // CAN1_TX
    0,                  // CAN1_RX0
    0,                  // CAN1_RX1
    0,                  // CAN1_SCE
    0,                  // EXTI9_5
    (uint32_t *)TIM1_BRK_TIM9_IRQHandler, // Placeholder if needed
    (uint32_t *)TIM1_UP_TIM10_IRQHandler,
    (uint32_t *)TIM1_TRG_COM_TIM11_IRQHandler,
    (uint32_t *)TIM1_CC_IRQHandler,
    (uint32_t *)TIM2_IRQHandler,
    0,                  // TIM3
    0,                  // TIM4
    0,                  // I2C1_EV
    0,                  // I2C1_ER
    0,                  // I2C2_EV
    0,                  // I2C2_ER
    0,                  // SPI1
    0,                  // SPI2
    (uint32_t *)USART1_IRQHandler,
    0,                  // USART2
    0,                  // 
    0,                  // EXTI15_10
    0,                  // RTC_Alarm
    0,                  // OTG_FS_WKUP
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // DMA1_Stream7
    0,                  // 
    0,                  // SDIO
    0,                  // TIM5
    0,                  // SPI3
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // DMA2_Stream0
    0,                  // DMA2_Stream1
    0,                  // DMA2_Stream2
    0,                  // DMA2_Stream3
    0,                  // DMA2_Stream4
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // OTG_FS
    0,                  // DMA2_Stream5
    0,                  // DMA2_Stream6
    0,                  // DMA2_Stream7
    0,                  // USART6
    0,                  // I2C3_EV
    0,                  // I2C3_ER
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // 
    0,                  // SPI4
    0,                  // SPI5
};




void Reset_Handler(void)
{
    // Copy .data from FLASH to RAM
    uint32_t size = (uint32_t)&_edata - (uint32_t)&_sdata;
    uint8_t *pDst = (uint8_t*)&_sdata;
    uint8_t *pSrc = (uint8_t*)&_sidata;
    
    for(uint32_t i=0; i<size; i++)
    {
        *pDst++ = *pSrc++;
    }
    
    // Zero .bss
    size = (uint32_t)&_ebss - (uint32_t)&_sbss;
    pDst = (uint8_t*)&_sbss;
    for(uint32_t i=0; i<size; i++)
    {
        *pDst++ = 0;
    }
    
    // Call system init if required (skipping for now, we do it in main)
    // SystemInit();
    
    // Call main
    main();
}

void Default_Handler(void)
{
    while(1);
}
