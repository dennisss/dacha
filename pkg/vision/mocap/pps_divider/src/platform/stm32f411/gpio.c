#include "stm32f4xx.h"
#include "drivers.h"

// GPIO Modes
#define GPIO_MODE_INPUT     0
#define GPIO_MODE_OUTPUT    1
#define GPIO_MODE_AF        2
#define GPIO_MODE_ANALOG    3

// GPIO Output Type
#define GPIO_OTYPE_PP       0
#define GPIO_OTYPE_OD       1

// GPIO Speed
#define GPIO_SPEED_LOW      0
#define GPIO_SPEED_MEDIUM   1
#define GPIO_SPEED_FAST     2
#define GPIO_SPEED_HIGH     3

// GPIO Pull
#define GPIO_PUPDR_NONE     0
#define GPIO_PUPDR_UP       1
#define GPIO_PUPDR_DOWN     2

void GPIO_Init(void)
{
    // PA0 (PPS Input) -> TIM2_CH1 (AF1)
    // PA1 (FRAME)     -> TIM2_CH2 (AF1)
    // PA2 (STROBE)    -> TIM2_CH3 (AF1)
    // PA9 (TX)        -> USART1_TX (AF7)
    // PA10 (RX)       -> USART1_RX (AF7)
    
    // Configure PA0, PA1, PA9, PA10 as Alternate Function.
    // LEAVE PA2 AS INPUT (00) INITIALLY (High-Z).
    // Configure PA0, PA1, PA9, PA10 as Alternate Function.
    // LEAVE PA2 AS INPUT (00) INITIALLY (High-Z).
    GPIOA->MODER &= ~(GPIO_MODER_MODE0 | GPIO_MODER_MODE1 | GPIO_MODER_MODE2 | GPIO_MODER_MODE9 | GPIO_MODER_MODE10);
    GPIOA->MODER |=  (GPIO_MODER_MODE0_1 | GPIO_MODER_MODE1_1 |                GPIO_MODER_MODE9_1 | GPIO_MODER_MODE10_1);
    
    // High Speed for Pulse outputs and UART
    GPIOA->OSPEEDR |= (GPIO_OSPEEDR_OSPEED0 | GPIO_OSPEEDR_OSPEED1 | GPIO_OSPEEDR_OSPEED2 | GPIO_OSPEEDR_OSPEED9 | GPIO_OSPEEDR_OSPEED10);
    
    // AF selection
    // PA0 -> AF1 (TIM2_CH1)
    // PA1 -> AF1 (TIM2_CH2)
    // PA2 -> AF1 (TIM2_CH3)
    GPIOA->AFR[0] &= ~((0xF << (0 * 4)) | (0xF << (1 * 4)) | (0xF << (2 * 4)));
    GPIOA->AFR[0] |=  ((1 << (0 * 4)) | (1 << (1 * 4)) | (1 << (2 * 4)));
    
    // PA9  -> AF7 (USART1_TX)
    // PA10 -> AF7 (USART1_RX)
    GPIOA->AFR[1] &= ~((0xF << ((9 - 8) * 4)) | (0xF << ((10 - 8) * 4)));
    GPIOA->AFR[1] |=  ((7 << ((9 - 8) * 4)) | (7 << ((10 - 8) * 4)));
    
    // PC13 (LED) -> Output
    GPIOC->MODER &= ~(GPIO_MODER_MODE13);
    GPIOC->MODER |=  (GPIO_MODER_MODE13_0);
    
    // Default LED off (High for Active Low)
    GPIOC->ODR |= GPIO_ODR_OD13;
}
