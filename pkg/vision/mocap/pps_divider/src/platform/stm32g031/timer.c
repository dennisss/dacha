#include "stm32g0xx.h"
#include "drivers.h"

// Helper function to be called from Application Logic
// We will define weak aliases or externs for callbacks.
void PPS_Input_Callback(uint32_t capture_val) __attribute__((weak));
void Frame_Output_Callback(void) __attribute__((weak));
void Strobe_Output_Callback(void) __attribute__((weak));

void PPS_Input_Callback(uint32_t capture_val) { (void)capture_val; }
void Frame_Output_Callback(void) { }
void Strobe_Output_Callback(void) { }

void Timer_Init(void)
{
    // Enable TIM2 Clock (Done in RCC)
    
    // Prescaler
    // F_timer = 64 MHz.
    // We want 64MHz counter ticker -> PSC = 0.
    TIM2->PSC = 0;
    
    // Auto-reload - Max
    TIM2->ARR = 0xFFFFFFFF;
    
    // --- Channel 2: PPS Input (PB3) ---
    // CC2S = 01 (Input on TI2)
    // --- Channel 2: PPS Input (PB3) ---
    // CC2S = 01 (Input on TI2)
    TIM2->CCMR1 &= ~TIM_CCMR1_CC2S; // Clear CC2S
    TIM2->CCMR1 |=  TIM_CCMR1_CC2S_0; // CC2S = 01
    
    // Enable Capture CC2E
    // CC2P=0 (Rising), CC2NP=0
    TIM2->CCER |= TIM_CCER_CC2E;
    
    // --- Channel 1: Frame Output (PA15) ---
    // CC1S = 00 (Output)
    TIM2->CCMR1 &= ~TIM_CCMR1_CC1S;
    // Mode: Frozen (000)
    TIM2->CCMR1 &= ~TIM_CCMR1_OC1M;
    
    // Enable Output CC1E
    TIM2->CCER |= TIM_CCER_CC1E;
    
    // --- Channel 3: Strobe Output (PC6) ---
    // CC3S = 00 (Output)
    TIM2->CCMR2 &= ~TIM_CCMR2_CC3S;
    // Mode: Frozen (000)
    TIM2->CCMR2 &= ~TIM_CCMR2_OC3M;
    
    // Ensure Disabled initially (controlled by logic)
    TIM2->CCER &= ~TIM_CCER_CC3E;
    
    // Enable Interrupts: CC1IE, CC2IE, CC3IE
    TIM2->DIER |= (TIM_DIER_CC1IE | TIM_DIER_CC2IE | TIM_DIER_CC3IE);
    
    // Enable NVIC for TIM2
    NVIC_EnableIRQ(TIM2_IRQn);
    
    // Enable Timer
    TIM2->CR1 |= TIM_CR1_CEN;

    // --- TIM1 Init (10kHz PWM on PA8) ---
    // F_timer = 64 MHz, F_pwm = 10 kHz -> ARR = 6400 - 1 = 6399
    TIM1->PSC = 0;
    TIM1->ARR = 6399;
    
    // Configure CH1 for PWM Mode 1 (110)
    TIM1->CCMR1 &= ~TIM_CCMR1_CC1S;
    TIM1->CCMR1 &= ~TIM_CCMR1_OC1M;
    TIM1->CCMR1 |= (6U << 4); // PWM Mode 1
    TIM1->CCMR1 |= TIM_CCMR1_OC1PE;
    
    // Enable Output CC1E
    TIM1->CCER |= TIM_CCER_CC1E;
    
    // Main Output Enable (MOE) required for Advanced Timers like TIM1
    TIM1->BDTR |= TIM_BDTR_MOE;
    
    TIM1->CCR1 = 0; // Initial duty cycle 0
    
    // Enable Timer
    TIM1->CR1 |= TIM_CR1_CEN;
}

void TIM2_IRQHandler(void)
{
    // Check CC1 (Frame Output)
    if (TIM2->SR & TIM_SR_CC1IF)
    {
        TIM2->SR = ~TIM_SR_CC1IF; // Clear Flag
        Frame_Output_Callback();
    }
    
    // Check CC3 (Strobe Output)
    if (TIM2->SR & TIM_SR_CC3IF)
    {
        TIM2->SR = ~TIM_SR_CC3IF; // Clear Flag
        Strobe_Output_Callback();
    }

    // Check CC2 (Input Capture - PPS)
    if (TIM2->SR & TIM_SR_CC2IF)
    {
        uint32_t capture = TIM2->CCR2;
        TIM2->SR = ~TIM_SR_CC2IF; // Clear Flag
        PPS_Input_Callback(capture);
    }
}
