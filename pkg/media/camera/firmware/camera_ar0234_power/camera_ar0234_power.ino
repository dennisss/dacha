// Firmware for implementing the power up and down sequence for the AR0234
//
// See pkg/media/camera/boards/camera_ar0234/index.md for how to flash this.
//
// - LDOs can take up to 1ms to reach full power
// - The crystal will take up to 5ms to start up.
// - After the 1ms reset pulse is sent, ~7ms (~160000 EXTCLK cycles) are
//   required for initialization before the sensor is ready for
//   communication.

#include <Arduino.h>

#define PIN_ENABLE_3V3    PIN_PA0
#define PIN_RESET_1V8     PIN_PA1
#define PIN_ENABLE_1V8    PIN_PA2
#define PIN_EXTCLK_ENABLE PIN_PA3
#define PIN_ENABLE_2V8    PIN_PA6
#define PIN_ENABLE_1V2    PIN_PA7

#define DEBOUCE_DELAY_US 200

bool is_powered_on = false;

// Helper to check if a pin stays mostly at a target state for a duration (noise-tolerant)
bool read_debounced(uint8_t pin, uint8_t target_state, uint32_t duration_us) {
    uint8_t matches = 0;
    uint32_t delay_step = duration_us / 20;
    
    for (int i = 0; i < 20; i++) {
        if (digitalRead(pin) == target_state) {
            matches++;
        }
        delayMicroseconds(delay_step);
    }
    
    // Require at least 80% of samples to match the target state to tolerate brief glitches
    return matches >= 16;
}

void power_on_sequence() {
    digitalWrite(PIN_ENABLE_2V8, HIGH);

    delayMicroseconds(200);

    digitalWrite(PIN_ENABLE_1V8, HIGH);

    delayMicroseconds(200);

    digitalWrite(PIN_ENABLE_1V2, HIGH);

    // Wait 1ms for all the LDOs to stabilize.
    delay(1);

    // Externally pulled up so set to High-Z to enable the clock.
    pinMode(PIN_EXTCLK_ENABLE, INPUT);
    
    // Wait for clock to stabilize.
    delay(10);

    // Reset needs to be pulsed low for at least 1ms
    digitalWrite(PIN_RESET_1V8, LOW);
    pinMode(PIN_RESET_1V8, OUTPUT);
    delay(2); //  2ms
    pinMode(PIN_RESET_1V8, INPUT);

    // Fix time for internal initialization 
    delay(10);
}

void power_off_sequence() {
    digitalWrite(PIN_EXTCLK_ENABLE, LOW);
    pinMode(PIN_EXTCLK_ENABLE, OUTPUT);
    
    delayMicroseconds(200);

    digitalWrite(PIN_ENABLE_1V2, LOW);

    delayMicroseconds(200);
    
    digitalWrite(PIN_ENABLE_1V8, LOW);
    
    delayMicroseconds(200);
    
    digitalWrite(PIN_ENABLE_2V8, LOW);

    // Prevent immediately powering back on (datasheet recommends at least 100ms).
    delay(100);
}

void setup() {
    pinMode(PIN_ENABLE_3V3, INPUT);

    // 1.8V capable pin. Pulled up externally.
    digitalWrite(PIN_RESET_1V8, LOW);
    pinMode(PIN_RESET_1V8, INPUT);

    digitalWrite(PIN_ENABLE_1V8, LOW);
    pinMode(PIN_ENABLE_1V8, OUTPUT);

    digitalWrite(PIN_EXTCLK_ENABLE, LOW);
    pinMode(PIN_EXTCLK_ENABLE, OUTPUT);

    digitalWrite(PIN_ENABLE_2V8, LOW);
    pinMode(PIN_ENABLE_2V8, OUTPUT);

    digitalWrite(PIN_ENABLE_1V2, LOW);
    pinMode(PIN_ENABLE_1V2, OUTPUT);

    // Currently we don't explicitly pull down the enable input pin so
    // wait some time after power up so that the Raspberry Pi reaches a
    // stable level for the pin.  
    delay(500);
}

void loop() {
    bool enable_state = digitalRead(PIN_ENABLE_3V3);

    if (!is_powered_on && enable_state == HIGH) {
        if (read_debounced(PIN_ENABLE_3V3, HIGH, DEBOUCE_DELAY_US)) {
            power_on_sequence();
            is_powered_on = true;
        }
    } else if (is_powered_on && enable_state == LOW) {
        if (read_debounced(PIN_ENABLE_3V3, LOW, DEBOUCE_DELAY_US)) {
            power_off_sequence();
            is_powered_on = false;
        }
    }
}
