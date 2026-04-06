#include "pico/stdlib.h"

#ifdef LOW_POWER_MODE
#include "pico/platform.h"
#include "hardware/powman.h"
#include "hardware/sync.h"
#include "hardware/irq.h"
#include "hardware/clocks.h"
#include "hardware/pll.h"
#include "hardware/structs/rosc.h"

void empty_powman_irq_handler() {
    powman_disable_alarm_wakeup();
    powman_clear_alarm();
}
#endif

void do_sleep_ms(uint32_t ms) {
#ifdef LOW_POWER_MODE
    uint64_t current_ms = powman_timer_get_ms();
    irq_set_exclusive_handler(POWMAN_IRQ_TIMER, empty_powman_irq_handler);
    irq_set_enabled(POWMAN_IRQ_TIMER, true);

    powman_enable_alarm_wakeup_at_ms(current_ms + ms);

    // 1. Safely switch clk_sys back to 12MHz XOSC so we can turn off the PLLs
    clock_configure(clk_sys,
                    CLOCKS_CLK_SYS_CTRL_SRC_VALUE_CLKSRC_CLK_SYS_AUX,
                    CLOCKS_CLK_SYS_CTRL_AUXSRC_VALUE_XOSC_CLKSRC,
                    12 * MHZ, 12 * MHZ);

    // 2. Instruct the hardware to completely power down both PLLs to save the ~3-4mA they burn
    pll_sys_hw->pwr = 0xffffffff;
    pll_usb_hw->pwr = 0xffffffff;
    
    // 3. Disable Ring Oscillator
    rosc_hw->ctrl = ROSC_CTRL_ENABLE_VALUE_DISABLE << ROSC_CTRL_ENABLE_LSB;

    __wfi();

    // 4. Re-enable Ring Oscillator
    rosc_hw->ctrl = ROSC_CTRL_ENABLE_VALUE_ENABLE << ROSC_CTRL_ENABLE_LSB;

    // 5. Hard re-initialize the System PLL to 150MHz (12MHz * 125 / 5 / 2 = 150MHz)
    pll_init(pll_sys, 1, 1500 * MHZ, 5, 2);

    // 6. Switch clk_sys back to the 150MHz PLL source
    clock_configure(clk_sys,
                    CLOCKS_CLK_SYS_CTRL_SRC_VALUE_CLKSRC_CLK_SYS_AUX,
                    CLOCKS_CLK_SYS_CTRL_AUXSRC_VALUE_CLKSRC_PLL_SYS,
                    150 * MHZ, 150 * MHZ);

    powman_disable_alarm_wakeup();
    irq_set_enabled(POWMAN_IRQ_TIMER, false);
    powman_clear_alarm();
#else
    sleep_ms(ms);
#endif
}

int main() {
    const uint LED_PIN = PICO_DEFAULT_LED_PIN;
    gpio_init(LED_PIN);
    gpio_set_dir(LED_PIN, GPIO_OUT);

    /*
    // Disable PFM mode on the DC/DC converter by setting GPIO23 high
    const uint DCDC_PFM_PIN = 23;
    gpio_init(DCDC_PFM_PIN);
    gpio_set_dir(DCDC_PFM_PIN, GPIO_OUT);
    gpio_put(DCDC_PFM_PIN, 1);
    */

#ifdef LOW_POWER_MODE
    // Initialize powman timer at 0
    powman_timer_set_ms(0);
    // Switch powman timer source to 1KHz XOSC (LPOSC may not tick unless explicitly started or configured properly)
    powman_timer_set_1khz_tick_source_xosc();
    powman_timer_start();
#endif

    while (true) {
        gpio_put(LED_PIN, 1);
        do_sleep_ms(1000);
        gpio_put(LED_PIN, 0);
        do_sleep_ms(1000);
    }
}
