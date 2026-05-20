#ifndef SYSTEM_H
#define SYSTEM_H

#include <stdint.h>
#ifdef STM32F411xE
#include "stm32f4xx.h"
#elif defined(STM32G031xx)
#include "stm32g0xx.h"
#endif

// System Clock Frequencies
#ifdef STM32G031xx
// G031 is configured for 64MHz (HSE 24MHz -> PLL)
#define HSE_VALUE           24000000U
#define SYSCLK_FREQ         64000000U
#define HCLK_FREQ           SYSCLK_FREQ
#define PCLK1_FREQ          HCLK_FREQ
#define PCLK2_FREQ          HCLK_FREQ

#else
// STM32F411 Blackpill
#define HSI_VALUE           16000000U
#define HSE_VALUE           25000000U 
#define PLL_M               25
#define PLL_N               192
#define PLL_P               2
#define PLL_Q               4

// SYSCLK = ((HSE_VALUE / PLL_M) * PLL_N) / PLL_P = 96MHz
#define SYSCLK_FREQ         96000000U
#define HCLK_FREQ           SYSCLK_FREQ
#define PCLK1_FREQ          (HCLK_FREQ / 2) 
#define PCLK2_FREQ          HCLK_FREQ       
#endif

#endif
