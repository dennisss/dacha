#include "stm32g0xx.h"
#include "drivers.h"

// ADC Channels
// PB1 -> ADC_IN9
// VREFINT -> ADC_IN13 (Internal)
// TEMP -> ADC_IN12 (Internal) - Check Datasheet for specific G031 mapping

// Stats
static uint16_t min_vcc = 0xFFFF;
static uint16_t max_vcc = 0;
static uint16_t min_pb1 = 0xFFFF;
static uint16_t max_pb1 = 0;
static uint32_t avg_temp_adc = 0;

// Factory Calibration Data
// VREFINT_CAL: TS_CAL1: 0x1FFF 75A8, VREFINT_CAL: 0x1FFF 75AA
// Check G031 Reference Manual/Datasheet
#define VREFINT_CAL_ADDR ((uint16_t*)0x1FFF75AA)
#define TS_CAL1_ADDR     ((uint16_t*)0x1FFF75A8) // @30C
#define TS_CAL2_ADDR     ((uint16_t*)0x1FFF75CA) // @130C (Check if available on G031)

// Definitions
#define ADC_MAX_VALUE 4095
#define VREFINT_CAL_VREF 3000 // mV (3.0V at calibration)
#define TARGET_VREF_MV   3300 // mV (Target reporting reference)

void ADC_Init(void) {
    // 1. Enable ADC Clock
    RCC->APBENR2 |= RCC_APBENR2_ADCEN;

    // 2. Configure ADC Clock Source
    // Default is asynchronous clock. We use synchronous PCLK/2 = 32MHz (Max is 35MHz).
    ADC1->CFGR2 |= ADC_CFGR2_CKMODE_0; // 01 for PCLK/2
    
    // 3. Enable Voltage Regulator
    // Required before calibration or enable on STM32G0
    ADC1->CR |= ADC_CR_ADVREGEN;
    for (volatile uint32_t i = 0; i < 2000; i++); // Wait ~20us (tADCVREG_SETUP)

    // 4. Enable VREFINT and Temp Sensor
    ADC->CCR |= ADC_CCR_VREFEN | ADC_CCR_TSEN;
    
    // 5. Calibrate
    ADC1->CR |= ADC_CR_ADCAL;
    while (ADC1->CR & ADC_CR_ADCAL);
    
    // 4. Enable ADC
    ADC1->ISR |= ADC_ISR_ADRDY;
    ADC1->CR |= ADC_CR_ADEN;
    while (!(ADC1->ISR & ADC_ISR_ADRDY));
    
    // 5. Configure Sampling Time (Longer for internal channels)
    // SMP1 for first channels, SMP2 for others?
    // G031 has shared sampling time. 
    ADC1->SMPR |= ADC_SMPR_SMP1_0 | ADC_SMPR_SMP1_1 | ADC_SMPR_SMP1_2; // Max sampling
}

uint16_t ADC_Read(uint32_t channel) {
    // Select Channel
    ADC1->CHSELR = (1 << channel);
    
    // Clear flags
    ADC1->ISR |= ADC_ISR_EOC | ADC_ISR_EOS;
    
    // Start Conversion
    ADC1->CR |= ADC_CR_ADSTART;
    
    // Wait for EOC
    while (!(ADC1->ISR & ADC_ISR_EOC));
    
    return ADC1->DR;
}

extern volatile uint16_t g_pcb_revision;

void ADC_Process(void) {
    // 1. Measure Signals
    uint16_t adc_vref = ADC_Read(13); // VREFINT (CH13 on G031?) 
    uint16_t adc_temp = ADC_Read(12); // Temp (CH12?)
    
    uint16_t adc_pb1 = 0;
    if (g_pcb_revision >= 8) {
        adc_pb1 = ADC_Read(6);  // PA6 (CH6)
    } else {
        adc_pb1 = ADC_Read(9);  // PB1 (CH9)
    }
    
    // 2. Calculate VCC (in mV)
    // VREFINT_CAL / ADC_VREF = VREFINT_V / VCC
    // VCC = (VREFINT_CAL * 3000) / ADC_VREF
    // Note: VREFINT_CAL is raw ADC value at 3.0V
    uint16_t vref_cal = *VREFINT_CAL_ADDR;
    uint32_t vcc_mv = (uint32_t)vref_cal * VREFINT_CAL_VREF / adc_vref;
    
    // 3. Calculate Normalized PB1 (Relative to 3.3V)
    // Voltage_Input = (ADC_PB1 / 4095) * VCC
    // Normalized_ADC = (Voltage_Input / 3300) * 4095
    // Normalized_ADC = ((ADC_PB1 * VCC) / 4095 / 3300) * 4095
    // Normalized_ADC = (ADC_PB1 * VCC) / 3300
    uint32_t pb1_norm = ((uint32_t)adc_pb1 * vcc_mv) / TARGET_VREF_MV;
    if (pb1_norm > 4095) pb1_norm = 4095;
    
    // 4. Calculate Normalized VCC (Relative to 1.2V Ref or just raw mV?)
    // Protocol asks for u16 "min_vcc", "max_vcc".
    // Let's store them as voltage in 10mV units maybe? Or just raw ADC of VREFINT?
    // User asked "u16 values can be in ADC 12-bit units".
    // So let's return VCC as if it was read by a 3.3V ADC? 
    // Wait, VCC is the supply. 
    // "u16 min_vcc over last second".
    // If we want it in ADC units (0-4095) relative to what?
    // Let's convert to mV and map 0-3300mV to 0-4095.
    uint32_t vcc_norm = (vcc_mv * 4095) / TARGET_VREF_MV;
    if (vcc_norm > 4095) vcc_norm = 4095;

    // 5. Temp Calculation
    // Temp (C) = ((TS_CAL2 - TS_CAL1) / (130 - 30)) * (Data - TS_CAL1) + 30
    // But we need to correct Data for VCC first?
    // "The temperature sensor output voltage changes linearly with temperature."
    // Data is proportional to VCC.
    // Normalized_Data = Data * VCC / 3000 (since CAL is at 3.0V)
    // Normalized_Data = Data * VCC / 3000 (since CAL is at 3.0V)
    int32_t temp_norm_adc = ((uint32_t)adc_temp * vcc_mv) / VREFINT_CAL_VREF;
    
    if (avg_temp_adc == 0) {
        avg_temp_adc = temp_norm_adc;
    } else {
        avg_temp_adc = (avg_temp_adc * 15 + temp_norm_adc) / 16;
    }
    
    // Update Stats
    if (vcc_norm < min_vcc) min_vcc = vcc_norm;
    if (vcc_norm > max_vcc) max_vcc = vcc_norm;
    
    if (pb1_norm < min_pb1) min_pb1 = pb1_norm;
    if (pb1_norm > max_pb1) max_pb1 = pb1_norm;
}

void ADC_GetStats(uint8_t *temp_half_c, uint16_t *v_min, uint16_t *v_max, uint16_t *p_min, uint16_t *p_max) {
    // Convert avg_temp_adc to Celsius using Factory Calibration
    int32_t ts_cal1 = *TS_CAL1_ADDR;
    int32_t ts_cal2 = *TS_CAL2_ADDR;
    
    // Temp (C) = ((TS_CAL2 - TS_CAL1) / (130 - 30)) * (Data - TS_CAL1) + 30
    // To avoid floats, we multiply by 100 / (130 - 30) = 1
    int32_t temp_c = ((int32_t)avg_temp_adc - ts_cal1) * 100 / (ts_cal2 - ts_cal1) + 30; // deg C
    
    // Scale to half-degrees
    int32_t temp_half = temp_c * 2;
    if (temp_half < 0) temp_half = 0;
    if (temp_half > 255) temp_half = 255;
    
    *temp_half_c = (uint8_t)temp_half;
    *v_min = min_vcc;
    *v_max = max_vcc;
    *p_min = min_pb1;
    *p_max = max_pb1;
    
    // Reset Stats
    min_vcc = 0xFFFF;
    max_vcc = 0;
    min_pb1 = 0xFFFF;
    max_pb1 = 0;
}
