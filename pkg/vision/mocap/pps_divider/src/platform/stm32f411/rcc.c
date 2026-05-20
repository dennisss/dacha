#include "stm32f4xx.h"
#include "system.h"
#include "drivers.h"

void RCC_Init(void)
{
    // 1. Enable HSE
    // 1. Enable HSE
    RCC->CR |= RCC_CR_HSEON;
    while (!(RCC->CR & RCC_CR_HSERDY)); // Wait for HSERDY

    // 2. Configure Flash Latency
    // 96MHz needs 3 wait states (LATENCY = 3)
    // PRFTEN | ICEN | DCEN | LATENCY=3
    FLASH->ACR = FLASH_ACR_PRFTEN | FLASH_ACR_ICEN | FLASH_ACR_DCEN | FLASH_ACR_LATENCY_3WS; 

    // 3. Configure PLL
    // PLLM = 25, PLLN = 192, PLLP = 2 (00), PLLQ = 4
    // PLL Source = HSE (Bit 22 = 1)
    
    uint32_t pllcfgr = 0;
    pllcfgr |= (PLL_M << RCC_PLLCFGR_PLLM_Pos);
    pllcfgr |= (PLL_N << RCC_PLLCFGR_PLLN_Pos);
    pllcfgr |= (0 << RCC_PLLCFGR_PLLP_Pos); // PLLP = 2 (00)
    pllcfgr |= RCC_PLLCFGR_PLLSRC_HSE;
    pllcfgr |= (PLL_Q << RCC_PLLCFGR_PLLQ_Pos);
    
    RCC->PLLCFGR = pllcfgr;

    // 4. Enable PLL
    RCC->CR |= RCC_CR_PLLON;
    while (!(RCC->CR & RCC_CR_PLLRDY)); // Wait for PLLRDY

    // 5. Configure Prescalers
    // AHB = 1 (Sysclk)
    // APB1 = 2 (HCLK / 2) -> 48MHz. (Max 50MHz)
    // APB2 = 1 (HCLK / 1) -> 96MHz. (Max 100MHz)
    
    uint32_t cfgr = RCC->CFGR;
    cfgr &= ~((0xF << 4) | (0x7 << 10) | (0x7 << 13)); // Clear HPRE, PPRE1, PPRE2
    
    // HPRE (AHB) = 1 -> 0xxx (Default 0)
    // PPRE1 (APB1) = 2 -> 100 (4)
    // PPRE2 (APB2) = 1 -> 0xx (0)
    
    cfgr |= (4 << 10); // PPRE1 = DIV2
    RCC->CFGR = cfgr;

    // 6. Switch System Clock to PLL
    RCC->CFGR &= ~RCC_CFGR_SW;
    RCC->CFGR |= RCC_CFGR_SW_PLL;

    // 7. Wait for Switch Status
    while ((RCC->CFGR & RCC_CFGR_SWS) != RCC_CFGR_SWS_PLL);

    // 8. Enable Peripheral Clocks
    RCC->AHB1ENR |= RCC_AHB1ENR_GPIOAEN | RCC_AHB1ENR_GPIOCEN;
    RCC->APB1ENR |= RCC_APB1ENR_TIM2EN;
    RCC->APB2ENR |= RCC_APB2ENR_USART1EN | RCC_APB2ENR_SYSCFGEN;
}
