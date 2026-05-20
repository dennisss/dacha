#include "stm32f4xx.h"
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
    // 1. Enable TIM2 Clock
    RCC->APB1ENR |= RCC_APB1ENR_TIM2EN;

    // 2. Configure Time Base
    TIM2->PSC = 0; // 96MHz
    TIM2->ARR = 0xFFFFFFFF; // Max 32-bit
    
    // 3. Configure Channel 1 (PA0) as Input Capture
    // CC1S = 01 (Input TI1)
    TIM2->CCMR1 &= ~TIM_CCMR1_CC1S;
    TIM2->CCMR1 |=  TIM_CCMR1_CC1S_0;
    
    // Filter? No filter for PPS (usually clean).
    // Polarity? Rising Edge. CC1P=0, CC1NP=0.
    TIM2->CCER &= ~(TIM_CCER_CC1P | TIM_CCER_CC1NP);
    
    // Enable Capture
    TIM2->CCER |= TIM_CCER_CC1E;
    
    // 4. Configure Channel 2 (PA1 - FRAME) as Output Compare
    // CC2S = 00 (Output)
    TIM2->CCMR1 &= ~TIM_CCMR1_CC2S;
    
    // Mode: Frozen (000) initially. Will be changed in ISR.
    TIM2->CCMR1 &= ~TIM_CCMR1_OC2M; 
    
    // Enable Output
    TIM2->CCER |= TIM_CCER_CC2E;
    
    // 5. Configure Channel 3 (PA2 - STROBE) as Output Compare
    // CC3S = 00 (Output)
    TIM2->CCMR2 &= ~TIM_CCMR2_CC3S;
    
    // Mode: Frozen (000) initially.
    TIM2->CCMR2 &= ~TIM_CCMR2_OC3M;
    
    // Enable Output
    TIM2->CCER |= TIM_CCER_CC3E; // Keep enabled? User wants High-Z when inactive.
    // We will control CC3E dynamically in pll.c
    TIM2->CCER &= ~TIM_CCER_CC3E; // Ensure Disabled initially
    
    // 6. Enable Interrupts
    // CC1IE (Input Capture), CC2IE (Output Compare), CC3IE (Output Compare)
    TIM2->DIER |= (TIM_DIER_CC1IE | TIM_DIER_CC2IE | TIM_DIER_CC3IE);
    
    // Enable NVIC
    NVIC_EnableIRQ(TIM2_IRQn);
    
    // 7. Enable Counter
    TIM2->CR1 |= TIM_CR1_CEN;
}

void TIM2_IRQHandler(void)
{
    // Check CC2 (Frame Output)
    if (TIM2->SR & TIM_SR_CC2IF)
    {
        TIM2->SR = ~TIM_SR_CC2IF; // Clear Flag
        Frame_Output_Callback();
    }
    
    // Check CC3 (Strobe Output)
    if (TIM2->SR & TIM_SR_CC3IF)
    {
        TIM2->SR = ~TIM_SR_CC3IF; // Clear Flag
        Strobe_Output_Callback();
    }

    // Check CC1 (Input Capture - PPS)
    if (TIM2->SR & TIM_SR_CC1IF)
    {
        uint32_t capture = TIM2->CCR1;
        TIM2->SR = ~TIM_SR_CC1IF; // Clear Flag
        PPS_Input_Callback(capture);
    }
}
