#include "system.h"
#include <stdbool.h>
#include <limits.h>
#include "protocol.h"
#include "drivers.h"
#include "system.h" // For SYSCLK_FREQ (96MHz or 64MHz)

// TODO: Support different PPS rates.

// Constants
#define TICKS_PER_SEC SYSCLK_FREQ

// Global Configuration
volatile ConfigData_t pll_config;

// Internal State
// PLL State Structure
typedef struct {
    // Index of the next PPS pulse we will receive.
    // - 0 means we have not received any PPS pulses yet.
    // - 1 means we have just received a single PPS pulse.
    // - 2+ means we have gotten at least 2 PPS pulses so pps_width is valid.
    uint32_t next_pulse_index; // 0 initially

    // The time of the last PPS pulse we received.
    uint32_t last_pulse_time;

    // The width of the last PPS pulse we received.
    uint32_t pps_width;        // Initialized to MAX (or nominal 96M)

    // The error between the last pulse and the expected pulse.
    int32_t pll_error;         // Initialized to MAX
} PLLState_t;

volatile PLLState_t g_pll = {
    .next_pulse_index = 0,
    .last_pulse_time = 0, // This is effectively "current_base_time" for the PLL loop
    .pps_width = TICKS_PER_SEC, // Nominal start
    .pll_error = 0
};

// Output Pulse State Structure
typedef struct {
    uint32_t current_base_time;
    uint32_t current_cycle_width;
    bool pulse_pending;
    uint32_t pulse_position;
    uint32_t late_error_count;
    bool error_state; // If true, output is aborted until next PPS
} OutputPulseState;

volatile OutputPulseState g_frame_state = {0};
volatile OutputPulseState g_strobe_state = {0};

// Globals for simple telemetry signaling
volatile uint8_t send_telem_flag = 0;
volatile uint32_t telem_width = 0;
volatile int32_t telem_error = 0;
volatile uint32_t telem_output_errors = 0;
volatile uint8_t telem_config_sequence = 0;

// Configurations for Timer Channels
#ifdef STM32G031xx
    // G031: Frame on PA15 (TIM2_CH1), Strobe on PC6 (TIM2_CH3)
    #define FRAME_TIMER_CH  1
    #define STROBE_TIMER_CH 3
#else
    // F411: Frame on PA1 (TIM2_CH2), Strobe on PA2 (TIM2_CH3)
    #define FRAME_TIMER_CH  2
    #define STROBE_TIMER_CH 3
#endif

// Forward Declaration
void Update_Output_Pulse(volatile OutputPulseState* state, int timer_channel_index, bool compare_interrupt_fired);

// Helper: Get Register Pointers/Masks for a Channel
static void Get_Timer_Channel_Resources(int channel_index, volatile uint32_t** ccr, volatile uint32_t** ccmr, uint32_t* shift, uint32_t* ccer_bit)
{
     if (channel_index == 1) {
         *ccr = &TIM2->CCR1;
         *ccmr = &TIM2->CCMR1;
         *shift = 4; // OC1
         *ccer_bit = 0;
     } else if (channel_index == 2) {
         *ccr = &TIM2->CCR2;
         *ccmr = &TIM2->CCMR1;
         *shift = 12; // OC2
         *ccer_bit = 4;
     } else { // CH3 (and CH4 would be CCMR2 shift 12)
         *ccr = &TIM2->CCR3;
         *ccmr = &TIM2->CCMR2;
         *shift = 4; // OC3
         *ccer_bit = 8;
     }
}

// Helper: Force Channel to Inactive Level (Low) and Disable
static void Force_Channel_Inactive(int channel_index)
{
    volatile uint32_t* ccr;
    volatile uint32_t* ccmr;
    uint32_t shift;
    uint32_t ccer_bit;
    
    Get_Timer_Channel_Resources(channel_index, &ccr, &ccmr, &shift, &ccer_bit);
    
    // Disable first to update mode safely (though we want to drive low)
    TIM2->CCER &= ~(1 << ccer_bit);
    
    // Set Mode to 'Force Inactive' (100 -> 4)
    *ccmr &= ~(7 << shift);
    *ccmr |=  (4 << shift);
    
    // Re-enable to drive the pin Low (if in AF mode)
    TIM2->CCER |= (1 << ccer_bit);
}

void PLL_Configure(ConfigData_t *config)
{
    // Critical Section: Ensure we don't race with ISRs
    __disable_irq();

    // Simple changes are ones that don't require any phase / frequency shifts to the outputs.
    // (e.g. just changing pulse widths)
    //
    // These can be done easily without extra re-syncing of the output state.
    bool is_simple_change = true;
    {
        if (config->unlock) is_simple_change = false;
        if (config->pulse_rate != pll_config.pulse_rate) is_simple_change = false;
        if (config->frame_offset_ticks != pll_config.frame_offset_ticks) is_simple_change = false;
        if (config->strobe_offset_ticks != pll_config.strobe_offset_ticks) is_simple_change = false;
        
        if (config->frame_width_ticks != pll_config.frame_width_ticks) {
            if (config->frame_width_ticks == 0 || pll_config.frame_width_ticks == 0) {
                is_simple_change = false;
            }
        }
        
        if (config->strobe_width_ticks != pll_config.strobe_width_ticks) {
            if (config->strobe_width_ticks == 0 || pll_config.strobe_width_ticks == 0) {
                is_simple_change = false;
            }
        }
    }

    // Copy config
    pll_config = *config;
    
#ifdef STM32G031xx
    // Update TIM1 CH1 PWM duty cycle (0-65535 maps to 0-6400)
    TIM1->CCR1 = ((uint32_t)pll_config.strobe_dimming * 6400) / 65535;
#endif
    
    // Reset PLL logic to Pulse 0 state (Re-lock input) if requested
    if (config->unlock) {
        g_pll.next_pulse_index = 0;
    }

    if (is_simple_change) {
        __enable_irq();
        return;
    }

    // Graceful Abort of Outputs
    // This is done because phase or frequency changes will probably mess up where we
    // are in the cycle so it is easily to just stop the outputs and have them re-sync
    // on the next PPS pulse.

    // 1. Frame Output
    g_frame_state.error_state = true;
    g_frame_state.late_error_count = 0; // Reset error count on config
    g_frame_state.pulse_position = 0;
    g_frame_state.pulse_pending = false;
    Force_Channel_Inactive(FRAME_TIMER_CH);
    
    // 2. Strobe Output
    g_strobe_state.error_state = true;
    g_strobe_state.late_error_count = 0; // Reset error count on config
    g_strobe_state.pulse_position = 0;
    g_strobe_state.pulse_pending = false;
    Force_Channel_Inactive(STROBE_TIMER_CH);
    
    // Configure Strobe GPIO State
    // If Disabled, set to High-Z. If Enabled, set to Alternate Function.
    if (pll_config.pulse_rate > 0 && pll_config.strobe_width_ticks > 0) {
        // Enable: Set PA2 to Alternate Function.
        #ifdef STM32G031xx
             // G031 PC6 is AF2 for TIM2_CH3.
             GPIOC->MODER &= ~(GPIO_MODER_MODE6);
             GPIOC->MODER |=  (GPIO_MODER_MODE6_1);
             GPIOC->AFR[0] &= ~(0xF << (6 * 4));
             GPIOC->AFR[0] |=  (2U << (6 * 4)); 
        #else
             // F411 PA2 is AF1 for TIM2_CH3.
             GPIOA->MODER &= ~(GPIO_MODER_MODE2);
             GPIOA->MODER |=  (GPIO_MODER_MODE2_1);
             GPIOA->AFR[0] &= ~(0xF << (2 * 4));
             GPIOA->AFR[0] |=  (1U << (2 * 4));
        #endif
    } else {
        // Disable by Config: Set Pin to Input (00) -> High-Z
        #ifdef STM32G031xx
            GPIOC->MODER &= ~(GPIO_MODER_MODE6);
        #else
            GPIOA->MODER &= ~(GPIO_MODER_MODE2);
        #endif
        
        // Also ensure Timer Output is disabled for Strobe (redundant but safe)
        TIM2->CCER &= ~TIM_CCER_CC3E; // CC3E is bit 8
    }
    
    __enable_irq();
}

// Logic to update a specific output channel
void Update_Output_Pulse(volatile OutputPulseState* state, int channel_index, bool compare_interrupt_fired)
{
    // 1. Handle Interrupt/Completion of previous pulse step
    if (state->pulse_pending) {
        if (compare_interrupt_fired) {
            state->pulse_pending = false;
            state->pulse_position++;
        } else {
            // Waiting for interrupt, nothing to do.
            return;
        }
    }

    if (pll_config.pulse_rate == 0) return;

    // 2. Check for New Cycle (PPS arrived)
    // Only reset when the output level is currently low so that the previous pulse can finish.
    if (state->current_base_time != g_pll.last_pulse_time &&
        (state->pulse_position % 2) == 0) {
        // New Cycle!
        state->current_base_time = g_pll.last_pulse_time;
        state->current_cycle_width = g_pll.pps_width;
        state->pulse_position = 0;
        state->error_state = false; // Reset error state on new cycle
    }
    
    // If in error state, do nothing until next cycle
    if (state->error_state) {
        return;
    }

    bool is_rising = (state->pulse_position % 2) == 0;
    uint32_t pulse_index = state->pulse_position / 2;

    // TODO: Eventually comment this out, but this will require better handling of the reset
    // condition to ensure we don't try scheduling a pulse in the past when we do a
    // current_base_time reset. e.g. copy the pulse_position mod pulse_rate when we
    // sync to a new base_time.
    if (pulse_index == pll_config.pulse_rate) {
        return;
    }

    // 3. Scheduling Loop
    // Try to schedule the next event. If it's in the past, abort.
        
    uint32_t target_time = 0;

    
    // Calculate Target Time
    // Use 64-bit math to prevent overflow when multiplying sequence index by cycle width
    // This eliminates the accumulated integer division error of a pre-calculated interval
    uint32_t pulse_start = state->current_base_time + 
            (uint32_t)(((uint64_t)(pulse_index + 1) * state->current_cycle_width) / pll_config.pulse_rate);
    
    if (is_rising) {
        target_time = pulse_start;
        if (state == &g_strobe_state) {
            target_time += pll_config.strobe_offset_ticks;
        } else if (state == &g_frame_state) {
            target_time += pll_config.frame_offset_ticks;
        }
    } else {
        // Falling edge
        uint32_t width = 0;
        if (state == &g_strobe_state) {
            width = pll_config.strobe_width_ticks;
            // Strobe Fall = Pulse Start + Offset + Width
            target_time = pulse_start + pll_config.strobe_offset_ticks + width;
        } else {
            width = pll_config.frame_width_ticks;
            // Frame Fall = Pulse Start + Offset + Width
            target_time = pulse_start + pll_config.frame_offset_ticks + width;
        }
    }

    // Unconditionally schedule it first
    volatile uint32_t* ccr_reg;
    volatile uint32_t* ccmr_reg;
    uint32_t ccmr_shift;
    uint32_t ccer_bit;
    
    Get_Timer_Channel_Resources(channel_index, &ccr_reg, &ccmr_reg, &ccmr_shift, &ccer_bit);

    *ccr_reg = target_time;
    
    // Configure Mode
    // Clear OCxM (Bits 6:4 shifted)
    *ccmr_reg &= ~(7 << ccmr_shift);
    if (is_rising) {
        *ccmr_reg |= (1 << ccmr_shift); // 001: Set Active on Match
    } else {
        *ccmr_reg |= (2 << ccmr_shift); // 010: Set Inactive on Match
    }
    
    // Enable Channel
    TIM2->CCER |= (1 << ccer_bit);
    
    state->pulse_pending = true;

    // Check against current time after configuration
    uint32_t now = TIM2->CNT;
    int32_t diff = (int32_t)(target_time - now);
    
    int32_t margin = 10;    
    if (!(diff > margin && diff < TICKS_PER_SEC)) {
        // Past or too close! Abort.
        state->error_state = true;
        state->pulse_position = 0;
        state->late_error_count++;
        state->pulse_pending = false;
        
        // Disable Channel and Force Low
        Force_Channel_Inactive(channel_index);
    }
}

// TODO: Issue is that if we miss a pulse, we will entirely stick a cycle of frame outputs.

// Called from TIM2_IRQHandler when PPS captured
void PPS_Input_Callback(uint32_t capture_val)
{
    // SW Debounce
    if (g_pll.next_pulse_index > 0) {
        uint32_t delta = capture_val - g_pll.last_pulse_time;
        if (delta < (TICKS_PER_SEC / 4)) {
            return;
        }
    }

    if (g_pll.next_pulse_index == 0) {
        // Pulse #0
        g_pll.last_pulse_time = capture_val;
        g_pll.next_pulse_index++;
        g_pll.pll_error = INT_MAX;

    } else if (g_pll.next_pulse_index == 1) {
        // Pulse #1: First period
        // TODO: Reject widths that are outo f expected range.
        g_pll.pps_width = capture_val - g_pll.last_pulse_time;
        g_pll.pll_error = 0;
        g_pll.last_pulse_time = capture_val;
        g_pll.next_pulse_index++;
    } else {
        // Pulse > 1
        uint32_t expected = g_pll.last_pulse_time + g_pll.pps_width;
        int32_t error = (int32_t)(capture_val - expected);
        g_pll.pll_error = error;

        // Max error is 1 millisecond.
        int32_t max_error = (int32_t)(TICKS_PER_SEC / 1000);
        if (error > max_error) {
            error = max_error;
        } else if (error < -max_error) {
            error = -max_error;
        }

        int32_t correction = error / 4;
        if (correction == 0 && error != 0) {
            correction = (error > 0) ? 1 : -1;
        }

        g_pll.pps_width = (uint32_t)((int32_t)g_pll.pps_width + correction);
        g_pll.last_pulse_time = capture_val;
        // g_pll.next_pulse_index++;
    }
    
    uint32_t min_width = TICKS_PER_SEC - (TICKS_PER_SEC / 1000);
    uint32_t max_width = TICKS_PER_SEC + (TICKS_PER_SEC / 1000);
    if (g_pll.pps_width < min_width) {
        g_pll.pps_width = min_width;
    } else if (g_pll.pps_width > max_width) {
        g_pll.pps_width = max_width;
    }

    // Telemetry
    telem_width = g_pll.pps_width;
    telem_error = g_pll.pll_error;
    telem_output_errors = g_frame_state.late_error_count + g_strobe_state.late_error_count;
    telem_config_sequence = pll_config.sequence;
    send_telem_flag = 1;

    // Toggle Status LED (PC13) - Blackpill Only
#ifdef STM32F411xE
    GPIOC->ODR ^= GPIO_ODR_OD13;
#endif

    // Update Pulse Logic for new Frame
    Update_Output_Pulse(&g_frame_state, FRAME_TIMER_CH, false);
    Update_Output_Pulse(&g_strobe_state, STROBE_TIMER_CH, false);
}

void Frame_Output_Callback(void) {
    Update_Output_Pulse(&g_frame_state, FRAME_TIMER_CH, true);
}

void Strobe_Output_Callback(void) {
    Update_Output_Pulse(&g_strobe_state, STROBE_TIMER_CH, true);
}

void PLL_Init_Loop(void) {
    // Empty
}
