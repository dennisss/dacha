


/*

Chip is "tinyAVR 1-Series" Attiny412

https://github.com/SpenceKonde/megaTinyCore/blob/master/megaavr/libraries/megaTinyCore/examples/readTempVcc/readTempVcc.ino
https://github.com/SpenceKonde/megaTinyCore/blob/master/megaavr/extras/Ref_Analog.md

https://github.com/SpenceKonde/megaTinyCore/blob/master/megaavr/extras/Ref_Digital.md
https://github.com/SpenceKonde/megaTinyCore/blob/master/megaavr/extras/Ref_DirectPortManipulation.md
*/


#include <megaTinyCore.h>
#include <tinyNeoPixel_Static.h>

#define PIN_SHEET_TEMP      PIN_PA1
#define PIN_FAN_PWM_LED     PIN_PA2 // TCA0 : WO2
#define PIN_BED_TEMP        PIN_PA3
#define PIN_SERIAL_TXRX     PIN_PA6
#define PIN_FAN_TACH        PIN_PA7

#define DEVICE_ADDRESS      0xAB
#define SERIAL_TIMEOUT_MS   100


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
    uint8_t length = sizeof(ResponsePacket);
    uint8_t address = DEVICE_ADDRESS;
    uint8_t sequence;
    uint16_t chip_temperature;
    uint16_t sheet_temperature;
    uint16_t bed_temperature;
    uint16_t fan_speed; // In RPM
    uint8_t checksum;
};
#pragma pack(pop)

#define NUM_LEDS 1

#define REQUEST_BUFFER_SIZE sizeof(RequestPacket)

uint8_t request_buffer[REQUEST_BUFFER_SIZE];
unsigned long first_byte_time = 0;
uint8_t received_bytes = 0;

volatile uint32_t tach_pulse_count = 0;
unsigned long tach_last_measure_time = 0;
uint16_t last_measured_rpm = 0;

uint32_t current_led_color = 0;
uint8_t current_fan_duty_cycle = 0;

byte pixels[NUM_LEDS * 4];

tinyNeoPixel strip(NUM_LEDS, PIN_FAN_PWM_LED, NEO_GRBW + NEO_KHZ800, pixels);


void setup() {
  // Setup serial to the host.
  Serial.swap(0); // Default pin assignment
  Serial.begin(115200, (SERIAL_8N1 | SERIAL_HALF_DUPLEX));

  // Start up the voltage references for ADC0 to avoid first measurement errors.
  VREF.CTRLB |= VREF_ADC0REFEN_bm;
  delay(10);

  // Configure input analog pins.
  pinMode(PIN_SHEET_TEMP, INPUT);
  pinMode(PIN_BED_TEMP, INPUT);

// https://github.com/SpenceKonde/megaTinyCore/blob/master/megaavr/extras/TakingOverTCA0.md#example-3-high-speed-8-bit-pwm

  // Configuring the fan pin.
  setup_pwm();
  // Set initial fan speed to 0%
  update_fan_pwm(0);

  // Initialize the NeoPixel library
  strip.begin();
  strip.setBrightness(255); // Use full brightness, color values will dictate intensity
  update_led_color(0);

  // Configure fan tachometer interrupt.
  pinMode(PIN_FAN_TACH, INPUT_PULLUP);
  attachInterrupt(digitalPinToInterrupt(PIN_FAN_TACH), tach_isr, FALLING);
  tach_last_measure_time = millis();
}

void tach_isr() {
    tach_pulse_count++;
}

// Configures TCA0 to output 10Hz PWM using channel 2 (wired to PA2 aka WA2)
void setup_pwm() {
    pinMode(PIN_FAN_PWM_LED, OUTPUT);
    PORTMUX.CTRLC = PORTMUX_TCA02_DEFAULT_gc; // Default port (don't use the alt port)

    digitalWriteFast(PIN_FAN_PWM_LED, false);

    /*

    takeOverTCA0();
  
    // Use TCA0 in single-slope PWM mode
    // Enable compare channel 2 for WO2
    TCA0.SINGLE.CTRLB = TCA_SINGLE_WGMODE_SINGLESLOPE_gc | TCA_SINGLE_CMP2EN_bm;

    // Set PWM frequency to 10Hz
    // F_CPU = 20,000,000 Hz
    // Prescaler = 256
    // PER = (F_CPU / (Prescaler * F_PWM)) - 1
    // PER = (20,000,000 / (256 * 10)) - 1 = 7812.5 - 1 = 7811.5 -> 7812
    TCA0.SINGLE.PER = 7812;
    
    // Set initial duty cycle to 0%
    TCA0.SINGLE.CMP2 = 0;

    // Set prescaler to 256 and enable the timer
    TCA0.SINGLE.CTRLA = TCA_SINGLE_CLKSEL_DIV256_gc | TCA_SINGLE_ENABLE_bm;
    */
}

/**
 * @brief Updates the fan PWM duty cycle.
 * @param duty_cycle A value from 0-255 representing 0-100% duty.
 */
void update_fan_pwm(uint8_t duty_cycle) {
    digitalWriteFast(PIN_FAN_PWM_LED, duty_cycle != 0? true : false);

    // TODO: We currently don't use PWM since the fan doesn't seem to like PWM and the LED seems to glitch off when turning off the fan with PWM
    
    /*
    // Enable the timer output on the pin in case the LED function disabled it.
    TCA0.SINGLE.CTRLB |= TCA_SINGLE_CMP2EN_bm; // Re-enable compare channel 1 output

    // Map the 0-255 duty cycle value to the timer's PERiod register value
    uint32_t cmp_value = (uint32_t)duty_cycle * TCA0.SINGLE.PER / 255;
    TCA0.SINGLE.CMP2 = (uint16_t)cmp_value;
    */
    
    current_fan_duty_cycle = duty_cycle;
}

/**
 * @brief Updates the WS2812 LED color. This temporarily disables PWM.
 * @param new_color 32-bit GRBW color value.
 */
void update_led_color(uint32_t new_color) {
    /*
    // Temporarily disable the PWM output on the pin
    TCA0.SINGLE.CTRLB &= ~TCA_SINGLE_CMP2EN_bm;
    */
    
    // RESET by holding low for >80us
    digitalWriteFast(PIN_FAN_PWM_LED, false);
    delayMicroseconds(100);

    // Send color data to the LED
    strip.setPixelColor(0, new_color);
    strip.show();

    // RESET by holding low for >80us
    digitalWriteFast(PIN_FAN_PWM_LED, false);
    delayMicroseconds(100);

    // Switch back to PWM output mode.
    update_fan_pwm(current_fan_duty_cycle);

    current_led_color = new_color;
}

void handle_request() {
  // Check if we could store the whole packet.
  if (received_bytes > REQUEST_BUFFER_SIZE) {
    return;
  }

  if (received_bytes != sizeof(RequestPacket)) {
    return;
  }
  
  RequestPacket* pkt = (RequestPacket*) request_buffer;

  if (pkt->address != DEVICE_ADDRESS) {
      return;
  }

  uint8_t expected_checksum = crc8(request_buffer, received_bytes - 1);
  if (expected_checksum != pkt->checksum) {
     return;
  }

  // Packet is valid. Do stuff now.

  // Update fan speed if it has changed
  if (pkt->desired_fan_speed != current_fan_duty_cycle) {
      update_fan_pwm(pkt->desired_fan_speed);
  }
  
  // Update LED color if it has changed
  if (pkt->desired_led_color != current_led_color) {
      update_led_color(pkt->desired_led_color);
  }

  analogReference(VDD);

  uint16_t sheet_temp = analogRead(PIN_SHEET_TEMP);
  uint16_t bed_temp = analogRead(PIN_BED_TEMP);

  noInterrupts();
  uint32_t pulses = tach_pulse_count;
  tach_pulse_count = 0;
  interrupts();

  unsigned long current_time = millis();
  unsigned long delta_t = current_time - tach_last_measure_time;
  tach_last_measure_time = current_time;

  if (delta_t > 0) {
      // RPM = (pulses / pulses_per_rev) / (time_in_minutes)
      // RPM = (pulses / 2) / (delta_t / 1000 / 60)
      // RPM = (pulses / 2) * (60000 / delta_t)
      // RPM = pulses * 30000 / delta_t
      last_measured_rpm = (uint16_t)((pulses * 30000UL) / delta_t);
  }


  ResponsePacket response;
  response.sequence = pkt->sequence;
  response.chip_temperature = readTemp();
  response.sheet_temperature = sheet_temp;
  response.bed_temperature = bed_temp;
  response.fan_speed = last_measured_rpm;
  response.checksum = crc8((uint8_t*)&response, sizeof(ResponsePacket) - 1);

  Serial.write((uint8_t*)&response, sizeof(ResponsePacket));
  Serial.flush();
}

uint8_t crc8(const uint8_t* data, size_t len) {
    const uint8_t INIT_REMAINDER = 0xFF;
    const uint8_t FINAL_XOR = 0x00;
    const uint8_t POLYNOMIAL = 0x31;

    uint8_t state = INIT_REMAINDER;

    for (size_t i = 0; i < len; ++i) {
        state ^= data[i];
        for (int j = 0; j < 8; ++j) {
            bool overflow = (state & 0x80) != 0; // Check if the MSB is 1
            state <<= 1;
            if (overflow) {
                state ^= POLYNOMIAL;
            }
        }
    }

    return state ^ FINAL_XOR;
}


void loop() {
  // Check for incoming serial data
  while (Serial.available() > 0) {
      if (received_bytes == 0) {
        first_byte_time = millis();
      }

      uint8_t b = Serial.read();
      if (received_bytes < REQUEST_BUFFER_SIZE) {
        request_buffer[received_bytes] = b;
      }
      received_bytes++;

      // Check if we read a whole packet.
      if (received_bytes > 0 && received_bytes == request_buffer[0]) {
        handle_request();
        received_bytes = 0;
      }
  }
  
  // Clear the packet if it is taking too long to be completed.
  if (received_bytes > 0 && (millis() - first_byte_time > SERIAL_TIMEOUT_MS)) {
      received_bytes = 0;
  }
}
