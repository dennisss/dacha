#include "stm32g0xx.h"
#include "drivers.h"

void GPIO_Init(void)
{
    // Enable GPIO Clocks (Already done in RCC_Init, but good practice to ensure)
    // RCC->IOPENR |= RCC_IOPENR_GPIOAEN | RCC_IOPENR_GPIOBEN | RCC_IOPENR_GPIOCEN;

    // --- USART1 Pins (PA9=TX, PA10=RX) ---
    // Remap PA11/12 to PA9/10 (Required for 28-pin G031 packages)
    SYSCFG->CFGR1 |= SYSCFG_CFGR1_PA11_RMP | SYSCFG_CFGR1_PA12_RMP;

    // AF Mode (10) for PA9, PA10
    GPIOA->MODER &= ~(GPIO_MODER_MODE9 | GPIO_MODER_MODE10);
    GPIOA->MODER |=  (GPIO_MODER_MODE9_1 | GPIO_MODER_MODE10_1);
    
    // AF1 (USART1) for PA9, PA10. (AFRH is AFR[1])
    // PA9 -> AF1 (USART1)
    // PA10 -> AF1 (USART1)
    GPIOA->AFR[1] &= ~((0xF << ((9 - 8) * 4)) | (0xF << ((10 - 8) * 4)));
    GPIOA->AFR[1] |=  ((1U << ((9 - 8) * 4)) | (1U << ((10 - 8) * 4)));

    // --- TIM1 Pins ---
    // PA8 -> TIM1_CH1 (PWM Output)
    GPIOA->MODER &= ~(GPIO_MODER_MODE8);
    GPIOA->MODER |=  (GPIO_MODER_MODE8_1); // AF Mode
    GPIOA->AFR[1] &= ~(0xF << ((8 - 8) * 4));
    GPIOA->AFR[1] |=  (2U << ((8 - 8) * 4)); // AF2 (TIM1_CH1)

    // --- TIM2 Pins ---
    // PB3  -> TIM2_CH2 (PPS Input)
    // PC6  -> TIM2_CH3 (Strobe Output)
    // PA15 -> TIM2_CH1 (Frame Output)
    
    // 1. Configure PA15 (AF2)
    // PA15 Mode -> 10 (AF)
    GPIOA->MODER &= ~(GPIO_MODER_MODE15);
    GPIOA->MODER |=  (GPIO_MODER_MODE15_1);
    
    // PA15 AFR[1] -> AF2 (TIM2_CH1)
    GPIOA->AFR[1] &= ~(0xF << ((15 - 8) * 4));
    GPIOA->AFR[1] |=  (2U << ((15 - 8) * 4));

    // 2. Configure PC6 (AF2)
    // PC6 Mode -> 10 (AF)
    GPIOC->MODER &= ~(GPIO_MODER_MODE6);
    GPIOC->MODER |=  (GPIO_MODER_MODE6_1);

    // PC6 AFR[0] -> AF2 (TIM2_CH3)
    GPIOC->AFR[0] &= ~(0xF << (6 * 4));
    GPIOC->AFR[0] |=  (2U << (6 * 4));
    
    // 3. Configure PB3 (AF2)
    // PB3 Mode -> 10 (AF)
    GPIOB->MODER &= ~(GPIO_MODER_MODE3);
    GPIOB->MODER |=  (GPIO_MODER_MODE3_1);
    
    // PB3 AFR[0] -> AF2 (TIM2_CH2)
    GPIOB->AFR[0] &= ~(0xF << (3 * 4));
    GPIOB->AFR[0] |=  (2U << (3 * 4));

    // 4. Configure Analog Inputs (ADC)
    // PB1 Mode -> 11 (Analog)
    GPIOB->MODER |= (GPIO_MODER_MODE1_0 | GPIO_MODER_MODE1_1);
    // PA6 Mode -> 11 (Analog)
    GPIOA->MODER |= (GPIO_MODER_MODE6_0 | GPIO_MODER_MODE6_1);

    // 5. Configure WS2812 SPI (PA7)
    // PA7 Mode -> 10 (AF)
    GPIOA->MODER &= ~(GPIO_MODER_MODE7);
    GPIOA->MODER |=  (GPIO_MODER_MODE7_1);
    // PA7 AFR[0] -> AF0 (SPI1_MOSI) - already 0 after reset, but let's clear it
    GPIOA->AFR[0] &= ~(0xF << (7 * 4));
}
