#include <stdint.h>

#define RCC_BASE      0x40023800
#define GPIOC_BASE    0x40020800

#define RCC_AHB1ENR   (*(volatile uint32_t *)(RCC_BASE + 0x30))
#define GPIOC_MODER   (*(volatile uint32_t *)(GPIOC_BASE + 0x00))
#define GPIOC_BSRR    (*(volatile uint32_t *)(GPIOC_BASE + 0x18))

extern uint32_t _estack;
extern uint32_t _sidata, _sdata, _edata;
extern uint32_t _sbss, _ebss;

void Reset_Handler(void);
int main(void);

__attribute__((section(".isr_vector")))
void (*const vector_table[])(void) = {
    (void (*)(void))(&_estack),
    Reset_Handler,
};

void Reset_Handler(void) {
    uint32_t *src = &_sidata;
    uint32_t *dst = &_sdata;
    while (dst < &_edata) {
        *dst++ = *src++;
    }

    dst = &_sbss;
    while (dst < &_ebss) {
        *dst++ = 0;
    }

    main();
    while (1) {}
}

void delay(uint32_t count) {
    for (uint32_t i = 0; i < count; i++) {
        __asm__ volatile ("nop");
    }
}

int main(void) {
    // 1. Enable GPIOC clock (bit 2 in AHB1ENR)
    RCC_AHB1ENR |= (1 << 2);

    // 2. Set PC13 to output mode (bits 27:26 = 01 in MODER)
    // Clear bits 27:26 first
    GPIOC_MODER &= ~(3U << 26);
    // Set bit 26
    GPIOC_MODER |= (1U << 26);

    while (1) {
        // Toggle PC13
        // To turn LED ON: set PC13 to LOW -> write to BSRR bit 29 (13 + 16)
        GPIOC_BSRR = (1 << 29);
        delay(2000000);

        // To turn LED OFF: set PC13 to HIGH -> write to BSRR bit 13
        GPIOC_BSRR = (1 << 13);
        delay(2000000);
    }

    return 0;
}
