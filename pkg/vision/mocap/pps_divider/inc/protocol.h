#ifndef PROTOCOL_H
#define PROTOCOL_H

#include <stdint.h>

#define SYNC_BYTE_1 0xAA
#define SYNC_BYTE_2 0x55

#define PKT_TYPE_CONFIG       0x01
#define PKT_TYPE_ACK          0x02
#define PKT_TYPE_TELEM        0x03
#define PKT_TYPE_HEARTBEAT    0x04
#define PKT_TYPE_GET_BUILD_ID 0x05
#define PKT_TYPE_BUILD_ID     0x06
#define PKT_TYPE_SETUP        0x07

typedef struct __attribute__((packed)) {
    uint8_t  sequence;
    // If non-zero, set g_pll.next_pulse_index = 0, else keep the old lock state. 
    uint8_t unlock;
    uint8_t  pulse_rate;
    uint32_t frame_width_ticks;   
    int32_t  frame_offset_ticks;
    int32_t  strobe_offset_ticks; 
    uint32_t strobe_width_ticks;
    uint16_t strobe_dimming;
    uint32_t rgb_color;
} ConfigData_t;

typedef struct __attribute__((packed)) {
    uint8_t sequence;
    uint8_t status;      
} AckData_t;

typedef struct __attribute__((packed)) {
    uint8_t sequence;
} GetBuildIdData_t;

typedef struct __attribute__((packed)) {
    uint8_t  sequence;
    uint64_t build_id;
} BuildIdData_t;

typedef struct __attribute__((packed)) {
    uint8_t  sequence;
    uint16_t pcb_revision;
} SetupData_t;

typedef struct __attribute__((packed)) {
    uint32_t pps_width;
    int32_t  pll_error;
    // Total number of errors encountered since last configured
    // (sum of all OutputPulseState::late_error_count fields)
    uint32_t output_errors;
    
    // Sequence number of the ConfigData currently in use.
    // (zero if not configured)
    uint8_t config_sequence;
    
    // System clock frequency
    uint8_t frequency_mhz;
} TelemetryData_t;

typedef struct __attribute__((packed)) {
    uint8_t  temp_c_half_deg; // Temperature in half-degrees Celsius (0 = 0C, 1 = 0.5C)
    uint16_t vcc_min_adc;    // Min VCC (relative to 1.2V int ref)
    uint16_t vcc_max_adc;    // Max VCC (in mV)
    uint16_t poe_min_adc;    // Min POE (calibrated to 3.3V ref)
    uint16_t poe_max_adc;    // Max POE (calibrated to 3.3V ref)
} HeartbeatPayload_t;

// Public API
void Protocol_Init(void);
void Protocol_ProcessByte(uint8_t byte);
void Protocol_SendTelemetry(uint32_t pps_width, int32_t pll_error, uint32_t output_errors, uint8_t config_sequence, uint8_t frequency_mhz);
void Protocol_SendHeartbeat(uint8_t temp, uint16_t vcc_min, uint16_t vcc_max, uint16_t poe_min, uint16_t poe_max);
void Protocol_SendAck(uint8_t sequence, uint8_t status);
void Protocol_SendBuildId(uint8_t sequence, uint64_t build_id);
void Protocol_CheckTimeout(void);

#endif
