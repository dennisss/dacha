#include <stdint.h>

extern uint32_t _estack;
extern uint32_t _sdata;
extern uint32_t _edata;
extern uint32_t _sidata;
extern uint32_t _sbss;
extern uint32_t _ebss;

extern void main(void);
void Reset_Handler(void);
void Default_Handler(void);

extern void main(void);
void Reset_Handler(void);
void Default_Handler(void);

// Weak aliases
void NMI_Handler(void) __attribute__((weak, alias("Default_Handler")));
void HardFault_Handler(void) __attribute__((weak, alias("Default_Handler")));
void SysTick_Handler(void) __attribute__((weak, alias("Default_Handler")));

void WWDG_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TIM2_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void USART1_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));

__attribute__((section(".isr_vector")))
const void * g_pfnVectors[] = {
  &_estack,
  Reset_Handler,
  NMI_Handler,
  HardFault_Handler,
  0,
  0,
  0,
  0,
  0,
  0,
  0, // Reserved
  0, // SVCall
  0, // Reserved
  0, // Reserved
  0, // PendSV
  SysTick_Handler,
  
  // External Interrupts
  WWDG_IRQHandler,          // IRQ 0
  Default_Handler,          // IRQ 1
  Default_Handler,          // IRQ 2 (RTC_TAMP)
  Default_Handler,          // IRQ 3 (FLASH)
  Default_Handler,          // IRQ 4 (RCC)
  Default_Handler,          // IRQ 5 (EXTI0_1)
  Default_Handler,          // IRQ 6 (EXTI2_3)
  Default_Handler,          // IRQ 7 (EXTI4_15)
  0,                        // IRQ 8 (Reserved)
  Default_Handler,          // IRQ 9 (DMA1_Channel1)
  Default_Handler,          // IRQ 10 (DMA1_Channel2_3)
  Default_Handler,          // IRQ 11 (DMA1_Ch4_5_DMAMUX1_OVR)
  Default_Handler,          // IRQ 12 (ADC1)
  Default_Handler,          // IRQ 13 (TIM1_BRK_UP_TRG_COM)
  Default_Handler,          // IRQ 14 (TIM1_CC)
  TIM2_IRQHandler,          // IRQ 15 (TIM2)
  Default_Handler,          // IRQ 16 (TIM3)
  Default_Handler,          // IRQ 17 (TIM6_DAC_LPTIM1)
  Default_Handler,          // IRQ 18 (TIM7_LPTIM2)
  Default_Handler,          // IRQ 19 (TIM14)
  Default_Handler,          // IRQ 20 (TIM15)
  Default_Handler,          // IRQ 21 (TIM16_FDCAN_IT0)
  Default_Handler,          // IRQ 22 (TIM17_FDCAN_IT1)
  Default_Handler,          // IRQ 23 (I2C1)
  Default_Handler,          // IRQ 24 (I2C2_3)
  Default_Handler,          // IRQ 25 (SPI1)
  Default_Handler,          // IRQ 26 (SPI2_3)
  USART1_IRQHandler,        // IRQ 27 (USART1)
};

void Reset_Handler(void)
{
  // Copy Data
  uint32_t *pSrc = &_sidata;
  uint32_t *pDest = &_sdata;
  while (pDest < &_edata) {
    *pDest++ = *pSrc++;
  }

  // Zero BSS
  pDest = &_sbss;
  while (pDest < &_ebss) {
    *pDest++ = 0;
  }

  main();
  
  while(1);
}

void Default_Handler(void)
{
  while(1);
}
