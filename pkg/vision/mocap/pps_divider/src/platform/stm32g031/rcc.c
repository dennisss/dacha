#include "stm32g0xx.h"
#include "drivers.h"

void RCC_Init(void)
{
    // 1. Enable HSE in Bypass Mode (for TCXO on PC14/OSC_IN)
    RCC->CR |= RCC_CR_HSEBYP;   // Set Bypass First
    RCC->CR |= RCC_CR_HSEON;    // Then Enable HSE
    while (!(RCC->CR & RCC_CR_HSERDY)); // Wait for HSERDY

    // 2. Configure Flash Latency
    // CPU = 64MHz -> 2 Wait States (Value 2)
    // FLASH_ACR_LATENCY_1 corresponds to bit 1 (0x2) which is value 2.
    // Use read-modify-write to preserve other bits like DBG_SWEN.
    FLASH->ACR = (FLASH->ACR & ~FLASH_ACR_LATENCY_Msk) | FLASH_ACR_LATENCY_1;
    // Wait until the new latency is set
    while ((FLASH->ACR & FLASH_ACR_LATENCY_Msk) != FLASH_ACR_LATENCY_1);

    // 3. Configure PLL
    // Source = HSE (24MHz)
    // Target = 64MHz
    // F_VCO_IN = 24 / M. Range 4-16MHz. Let M=3 -> 8MHz.
    // F_VCO_OUT = 8 * N. Range 64-344MHz. Let N=16 -> 128MHz.
    // F_PLL_R = 128 / R. Range 64MHz max. Let R=2 -> 64MHz.
    
    // Clear PLL config
    RCC->PLLCFGR = 0;
    
    // G031 PLLCFGR:
    // PLLSRC[1:0]: 11 (HSE) -> 3
    // PLLM[6:4]:   010 (3)  -> 2  (Input 24MHz / 3 = 8MHz)
    // PLLN[14:8]:  16       -> 16 (VCO 8MHz * 16 = 128MHz)
    // PLLR[31:29]: 000 (2)  -> 0  (Output 128MHz / 2 = 64MHz)
    // PLLREN[28]:  1        -> Enable
    
    uint32_t pllcfgr = 0;
    pllcfgr |= RCC_PLLCFGR_PLLSRC_HSE;
    pllcfgr |= (2 << RCC_PLLCFGR_PLLM_Pos);  // PLLM = 3. Value 2 (010).
    pllcfgr |= (16 << RCC_PLLCFGR_PLLN_Pos); // PLLN = 16
    pllcfgr |= (0 << RCC_PLLCFGR_PLLR_Pos);  // PLLR = 2. Value 0 (000).
    pllcfgr |= RCC_PLLCFGR_PLLREN;
    
    RCC->PLLCFGR = pllcfgr;

    // 4. Enable PLL
    RCC->CR |= RCC_CR_PLLON;
    while (!(RCC->CR & RCC_CR_PLLRDY));

    // 5. Switch System Clock to PLL
    // SW[1:0]: 00 (HSI), 01 (HSE), 10 (PLL), 11 (LSI)
    RCC->CFGR &= ~RCC_CFGR_SW;
    RCC->CFGR |= RCC_CFGR_SW_PLLRCLK;

    // 6. Wait for Switch
    while ((RCC->CFGR & RCC_CFGR_SWS) != RCC_CFGR_SWS_PLLRCLK);

    // 7. Enable Peripheral Clocks
    RCC->IOPENR |= RCC_IOPENR_GPIOAEN | RCC_IOPENR_GPIOBEN | RCC_IOPENR_GPIOCEN;
    RCC->APBENR1 |= RCC_APBENR1_TIM2EN;
    RCC->APBENR2 |= RCC_APBENR2_USART1EN | RCC_APBENR2_SYSCFGEN | RCC_APBENR2_TIM1EN;
}
