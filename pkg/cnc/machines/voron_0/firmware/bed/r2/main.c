#include "stm32c0xx.h"
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#define DEVICE_ADDRESS      0xAB
#define SERIAL_TIMEOUT_MS   100

// Optionally define this to enable full duplex USART
// #define USE_FULL_DUPLEX

#pragma pack(push, 1)
struct RequestPacket {
    uint8_t length;
    uint8_t address;
    uint8_t sequence;
    uint8_t desired_fan_speed;
    uint32_t desired_led_color;
    uint8_t checksum;
};

struct ResponsePacket {
    uint8_t length;
    uint8_t address;
    uint8_t sequence;
    uint16_t chip_temperature;
    uint16_t sheet_temperature;
    uint16_t bed_temperature;
    uint16_t fan_speed; // In RPM
    uint8_t checksum;
};
#pragma pack(pop)

#define REQUEST_BUFFER_SIZE sizeof(struct RequestPacket)

uint8_t request_buffer[REQUEST_BUFFER_SIZE];
volatile uint8_t received_bytes = 0;
volatile uint32_t first_byte_time = 0;

volatile uint32_t tach_pulse_count = 0;
uint32_t tach_last_measure_time = 0;
uint16_t last_measured_rpm = 0;

uint32_t current_led_color = 0;
uint8_t current_fan_duty_cycle = 0;

volatile uint32_t ms_ticks = 0;

void SysTick_Handler(void) {
    ms_ticks++;
}

uint32_t millis(void) {
    return ms_ticks;
}

uint8_t crc8(const uint8_t* data, size_t len) {
    const uint8_t INIT_REMAINDER = 0xFF;
    const uint8_t FINAL_XOR = 0x00;
    const uint8_t POLYNOMIAL = 0x31;

    uint8_t state = INIT_REMAINDER;

    for (size_t i = 0; i < len; ++i) {
        state ^= data[i];
        for (int j = 0; j < 8; ++j) {
            bool overflow = (state & 0x80) != 0;
            state <<= 1;
            if (overflow) {
                state ^= POLYNOMIAL;
            }
        }
    }

    return state ^ FINAL_XOR;
}

void SystemClock_Config(void) {
    // 48 MHz using HSI48
    // Enable HSI
    RCC->CR |= RCC_CR_HSION;
    while((RCC->CR & RCC_CR_HSIRDY) == 0);

    // 48MHz requires 1 wait state for flash. Also enable Instruction Cache.
    FLASH->ACR = (FLASH->ACR & ~FLASH_ACR_LATENCY_Msk) | FLASH_ACR_LATENCY_0 | FLASH_ACR_ICEN;
    while ((FLASH->ACR & FLASH_ACR_LATENCY_Msk) != FLASH_ACR_LATENCY_0);

    // Set HSIDIV to 1 (divide by 1)
    RCC->CR &= ~RCC_CR_HSIDIV_Msk;

    SystemCoreClock = 48000000;
    SysTick_Config(SystemCoreClock / 1000);
}

void GPIO_Init(void) {
    RCC->IOPENR |= RCC_IOPENR_GPIOAEN | RCC_IOPENR_GPIOBEN;

    // PA2 Output (LED WS2812)
    GPIOA->MODER &= ~GPIO_MODER_MODE2_Msk;
    GPIOA->MODER |= GPIO_MODER_MODE2_0; // Output
    GPIOA->OSPEEDR |= GPIO_OSPEEDR_OSPEED2_0 | GPIO_OSPEEDR_OSPEED2_1; // High speed

    // PB6 Input Pull-up (Tachometer)
    GPIOB->MODER &= ~GPIO_MODER_MODE6_Msk;
    GPIOB->PUPDR |= GPIO_PUPDR_PUPD6_0; // Pull-up

    // PB7 Alternate Function (TIM3_CH4)
    GPIOB->MODER &= ~GPIO_MODER_MODE7_Msk;
    GPIOB->MODER |= GPIO_MODER_MODE7_1;
    GPIOB->AFR[0] &= ~GPIO_AFRL_AFSEL7_Msk;
    GPIOB->AFR[0] |= (3 << GPIO_AFRL_AFSEL7_Pos); // AF3 is TIM3_CH4
}

void EXTI_Init(void) {
    RCC->APBENR2 |= RCC_APBENR2_SYSCFGEN;
    EXTI->EXTICR[1] = (EXTI->EXTICR[1] & ~EXTI_EXTICR2_EXTI6_Msk) | (1 << EXTI_EXTICR2_EXTI6_Pos); // PB6
    EXTI->IMR1 |= EXTI_IMR1_IM6; // Unmask EXTI6
    EXTI->FTSR1 |= EXTI_FTSR1_FT6; // Falling edge
    NVIC_EnableIRQ(EXTI4_15_IRQn);
}

void EXTI4_15_IRQHandler(void) {
    if (EXTI->RPR1 & EXTI_RPR1_RPIF6) {
        EXTI->RPR1 = EXTI_RPR1_RPIF6;
    }
    if (EXTI->FPR1 & EXTI_FPR1_FPIF6) {
        EXTI->FPR1 = EXTI_FPR1_FPIF6;
        tach_pulse_count++;
    }
}

void USART1_Init(void) {
    RCC->APBENR2 |= RCC_APBENR2_USART1EN;

    // PA0 -> AF4 (USART1_TX)
    GPIOA->MODER &= ~GPIO_MODER_MODE0_Msk;
    GPIOA->MODER |= (2 << GPIO_MODER_MODE0_Pos);
    GPIOA->AFR[0] &= ~GPIO_AFRL_AFSEL0_Msk;
    GPIOA->AFR[0] |= (4 << GPIO_AFRL_AFSEL0_Pos);

#ifdef USE_FULL_DUPLEX
    // PA1 -> AF4 (USART1_RX)
    GPIOA->MODER &= ~GPIO_MODER_MODE1_Msk;
    GPIOA->MODER |= (2 << GPIO_MODER_MODE1_Pos);
    GPIOA->AFR[0] &= ~GPIO_AFRL_AFSEL1_Msk;
    GPIOA->AFR[0] |= (4 << GPIO_AFRL_AFSEL1_Pos);
#endif

    USART1->CR1 = 0;
    // Baud rate 115200. USART clock = 48MHz
    USART1->BRR = 48000000 / 115200; // 417
    
    // Enable RXNE interrupt
    USART1->CR1 |= USART_CR1_RXNEIE_RXFNEIE;

#ifndef USE_FULL_DUPLEX
    // Half duplex mode
    USART1->CR3 |= USART_CR3_HDSEL;
#endif

    // Enable USART, TX, RX
    USART1->CR1 |= USART_CR1_UE | USART_CR1_TE | USART_CR1_RE;

    NVIC_EnableIRQ(USART1_IRQn);
}

void USART1_IRQHandler(void) {
    if (USART1->ISR & USART_ISR_RXNE_RXFNE) {
        uint8_t b = USART1->RDR;
        if (received_bytes == 0) {
            first_byte_time = millis();
        }
        if (received_bytes < REQUEST_BUFFER_SIZE) {
            request_buffer[received_bytes] = b;
        }
        received_bytes++;
    }
}

void ADC_Init(void) {
    RCC->APBENR2 |= RCC_APBENR2_ADCEN;

    // PA3 and PA12 as analog
    GPIOA->MODER |= GPIO_MODER_MODE3_Msk;
    GPIOA->MODER |= GPIO_MODER_MODE12_Msk;

    ADC1->CR |= ADC_CR_ADVREGEN;
    for(volatile int i=0; i<1000; i++);

    ADC1->CR |= ADC_CR_ADCAL;
    while(ADC1->CR & ADC_CR_ADCAL);

    // Set sampling time to 160.5 ADC clock cycles to allow the internal temperature sensor capacitor to charge
    ADC1->SMPR |= ADC_SMPR_SMP1;

    // Enable hardware oversampling for 16-bit max resolution (14.5 bits true resolution)
    // Ratio = 32x (OVSR = 100)
    // Shift = 1 bit right (OVSS = 0001). Max value = 32 * 4095 >> 1 = 65520 (Fits in uint16_t)
    // Also set ADC Clock to PCLK/2 (24MHz) to ensure clean analog performance & stay within 35MHz spec
    ADC1->CFGR2 |= ADC_CFGR2_OVSE | ADC_CFGR2_OVSR_2 | ADC_CFGR2_OVSS_0 | ADC_CFGR2_CKMODE_0;

    ADC1->CR |= ADC_CR_ADEN;
    while(!(ADC1->ISR & ADC_ISR_ADRDY));

    ADC1_COMMON->CCR |= ADC_CCR_TSEN | ADC_CCR_VREFEN;
    
    for(volatile int i=0; i<10000; i++); 
}

uint16_t adc_read(uint32_t channel) {
    ADC1->CHSELR = (1 << channel);
    ADC1->CR |= ADC_CR_ADSTART;
    while(!(ADC1->ISR & ADC_ISR_EOC));
    return ADC1->DR;
}

void TIM3_Init(void) {
    RCC->APBENR1 |= RCC_APBENR1_TIM3EN;
    // 48 MHz clock
    // Timer freq = 10Hz, ARR = 254 (Allows 100% duty cycle when CCR=255)
    // 48000000 / 18897.6 / 254 = 10Hz
    TIM3->PSC = 18898 - 1;
    TIM3->ARR = 254;
    TIM3->CCR4 = 0;
    
    // PWM Mode 1 on CH4
    TIM3->CCMR2 &= ~TIM_CCMR2_OC4M_Msk;
    TIM3->CCMR2 |= (6 << TIM_CCMR2_OC4M_Pos) | TIM_CCMR2_OC4PE;
    
    TIM3->CCER |= TIM_CCER_CC4E; // Enable CH4 output
    
    TIM3->CR1 |= TIM_CR1_CEN; // Enable counter
}

void update_fan_pwm(uint8_t duty) {
    TIM3->CCR4 = duty;
    current_fan_duty_cycle = duty;
}

// WS2812 NOPs for 48MHz (Tuned: 20 NOPs = ~660ns)
#define NOP1() __NOP()
#define NOP5() NOP1();NOP1();NOP1();NOP1();NOP1()
#define NOP10() NOP5();NOP5()
#define NOP25() NOP10();NOP10();NOP5()

static inline void ws2812_send_bit(int bit) {
    if (bit) {
        GPIOA->BSRR = GPIO_BSRR_BS2;
        NOP25(); // T1H ~ 825ns
        GPIOA->BSRR = GPIO_BSRR_BR2;
        NOP10(); // T1L ~ 330ns + loop overhead
    } else {
        GPIOA->BSRR = GPIO_BSRR_BS2;
        NOP10(); // T0H ~ 330ns
        GPIOA->BSRR = GPIO_BSRR_BR2;
        NOP25(); // T0L ~ 825ns + loop overhead
    }
}

void update_led_color(uint32_t grbw) {
    uint8_t w = (grbw >> 24) & 0xFF;
    uint8_t r = (grbw >> 16) & 0xFF;
    uint8_t g = (grbw >> 8) & 0xFF;
    uint8_t b = grbw & 0xFF;

    // Pack into wire order: Green, Red, Blue, White
    uint32_t wire_data = (g << 24) | (r << 16) | (b << 8) | w;

    __disable_irq();
    for (int i = 31; i >= 0; i--) {
        ws2812_send_bit((wire_data >> i) & 1);
    }
    __enable_irq();
    current_led_color = grbw;
}

void uart_send(const uint8_t* data, size_t len) {
#ifndef USE_FULL_DUPLEX
    USART1->CR1 &= ~USART_CR1_RE; // Disable RX
#endif

    for (size_t i = 0; i < len; i++) {
        while (!(USART1->ISR & USART_ISR_TXE_TXFNF));
        USART1->TDR = data[i];
    }
    while (!(USART1->ISR & USART_ISR_TC));
    USART1->ICR = USART_ICR_TCCF; // Clear TC

#ifndef USE_FULL_DUPLEX
    USART1->CR1 |= USART_CR1_RE; // Re-enable RX
#endif
}

// Calculates the true chip temperature in Celsius * 100
static uint16_t calculate_chip_temperature(uint16_t raw_adc) {
    // STM32C011 factory calibration value at 30°C (measured at VDD = 3.0V)
    uint16_t ts_cal1 = *((uint16_t*)0x1FFF7568);
    
    // The ADC reading is currently 32x oversampled and right-shifted by 1.
    // This makes it effectively a 16-bit value (16x larger than a 12-bit value).
    // Shift right by 4 to bring it back to the standard 12-bit scale for math.
    uint16_t ts_data = raw_adc >> 4;
    
    // Calculate absolute voltage in millivolts
    // The user's board uses an external 3.3V LDO, so VDD = 3.3V
    int32_t v_in_mv = ((int32_t)ts_data * 3300) / 4095;
    
    // Factory calibration was done at 3.0V
    int32_t v_30_mv = ((int32_t)ts_cal1 * 3000) / 4095;
    
    int32_t delta_v_mv = v_in_mv - v_30_mv;
    
    // STM32C011 Average Slope is ~2.53 mV/°C
    // delta_T = delta_V / 2.53
    // We want Temperature * 100, so we multiply by 10000 and divide by 253
    int32_t delta_t_100 = (delta_v_mv * 10000) / 253;
    
    // Base temperature is 30.00 °C (3000)
    int32_t temp_100 = 3000 + delta_t_100;
    
    // Clamp to positive range to fit gracefully into uint16_t
    if (temp_100 < 0) return 0;
    if (temp_100 > 65535) return 65535;
    
    return (uint16_t)temp_100;
}

void handle_request(void) {
    struct RequestPacket* pkt = (struct RequestPacket*) request_buffer;

    if (pkt->address != DEVICE_ADDRESS) {
        return;
    }

    uint8_t expected_checksum = crc8((uint8_t*)request_buffer, received_bytes - 1);
    if (expected_checksum != pkt->checksum) {
        return;
    }

    if (pkt->desired_fan_speed != current_fan_duty_cycle) {
        update_fan_pwm(pkt->desired_fan_speed);
    }
    
    if (pkt->desired_led_color != current_led_color) {
        update_led_color(pkt->desired_led_color);
    }

    uint16_t sheet_temp = adc_read(3); // PA3 = IN3
    uint16_t bed_temp = adc_read(12); // PA12 = IN12
    uint16_t raw_chip_adc = adc_read(9); // TSEN is channel 9 on STM32C011
    uint16_t chip_temp = calculate_chip_temperature(raw_chip_adc);

    __disable_irq();
    uint32_t pulses = tach_pulse_count;
    tach_pulse_count = 0;
    __enable_irq();

    uint32_t current_time = millis();
    uint32_t delta_t = current_time - tach_last_measure_time;
    tach_last_measure_time = current_time;

    if (delta_t > 0) {
        last_measured_rpm = (uint16_t)((pulses * 30000UL) / delta_t);
    }

    struct ResponsePacket response;
    response.length = sizeof(struct ResponsePacket);
    response.address = DEVICE_ADDRESS;
    response.sequence = pkt->sequence;
    response.chip_temperature = chip_temp;
    response.sheet_temperature = sheet_temp;
    response.bed_temperature = bed_temp;
    response.fan_speed = last_measured_rpm;
    response.checksum = crc8((uint8_t*)&response, sizeof(struct ResponsePacket) - 1);

    // Send response
    uart_send((const uint8_t*)&response, sizeof(struct ResponsePacket));
}

int main(void) {
    SystemClock_Config();
    GPIO_Init();
    EXTI_Init();
    USART1_Init();

    ADC_Init();
    TIM3_Init();

    // Set initial state
    update_fan_pwm(0);
    update_led_color(0);

    tach_last_measure_time = millis();

    while (1) {
        // Wait for packet
        __disable_irq();
        uint8_t len = received_bytes;
        __enable_irq();

        if (len > 0 && len == request_buffer[0]) {
            handle_request();
            __disable_irq();
            received_bytes = 0;
            __enable_irq();
        }

        if (len > 0 && (millis() - first_byte_time > SERIAL_TIMEOUT_MS)) {
            __disable_irq();
            received_bytes = 0;
            __enable_irq();
        }
    }
}
