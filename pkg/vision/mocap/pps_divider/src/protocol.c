#include "protocol.h"
#include "drivers.h"
#include "crc.h"
#include <string.h>
#include <stdbool.h>

#define MAX_PAYLOAD_LEN 64

typedef enum {
    STATE_SYNC1,
    STATE_SYNC2,
    STATE_LEN,
    STATE_TYPE,
    STATE_DATA,
    STATE_CRC1,
    STATE_CRC2,
    STATE_CRC3,
    STATE_CRC4
} ParserState_t;

static ParserState_t state = STATE_SYNC1;
static uint8_t msg_len = 0;
static uint8_t msg_type = 0;
static uint8_t msg_data[MAX_PAYLOAD_LEN];
static uint8_t msg_idx = 0;
static uint32_t rx_crc = 0;
static uint32_t last_byte_time = 0;
static uint8_t current_seq = 0;

extern volatile uint32_t ms_ticks;

extern void PLL_Configure(ConfigData_t *config); // Forward declaration
extern volatile const uint64_t g_build_id;
extern volatile uint16_t g_pcb_revision;
extern volatile bool g_setup_received;





void Protocol_Init(void)
{
    state = STATE_SYNC1;
}

void Protocol_ProcessByte(uint8_t byte)
{
    last_byte_time = ms_ticks;
    
    switch (state) {
        case STATE_SYNC1:
            if (byte == SYNC_BYTE_1) state = STATE_SYNC2;
            break;
            
        case STATE_SYNC2:
            if (byte == SYNC_BYTE_2) state = STATE_LEN;
            else if (byte == SYNC_BYTE_1) state = STATE_SYNC2;
            else state = STATE_SYNC1;
            break;
            
        case STATE_LEN:
            msg_len = byte;
            // Len includes TYPE (1 byte) + DATA.
            if (msg_len < 1 || msg_len > MAX_PAYLOAD_LEN) {
                state = STATE_SYNC1; // Invalid length
                Protocol_SendAck(0, 4); // Status 4: Format Error
            } else {
                state = STATE_SYNC1; // Invalid length
                state = STATE_TYPE;
            }
            break;
            
        case STATE_TYPE:
            msg_type = byte;
            msg_idx = 0;
            if (msg_len == 1) { // No data
                state = STATE_CRC1;
            } else {
                state = STATE_DATA;
            }
            break;
            
        case STATE_DATA:
            msg_data[msg_idx++] = byte;
            if (msg_idx >= (msg_len - 1)) {
                state = STATE_CRC1;
            }
            break;
            
        case STATE_CRC1: rx_crc = byte; state = STATE_CRC2; break;
        case STATE_CRC2: rx_crc |= (byte << 8); state = STATE_CRC3; break;
        case STATE_CRC3: rx_crc |= (byte << 16); state = STATE_CRC4; break;
        case STATE_CRC4: 
            rx_crc |= (byte << 24); 
            
            // 1. Header (LEN, TYPE)
            uint8_t header[2] = {msg_len, msg_type};
            uint32_t calc_crc = CRC_Calculate(header, 2, CRC_INITIAL_VALUE);
            
            // 2. Payload
            if (msg_len > 1) {
                calc_crc = CRC_Calculate(msg_data, msg_len - 1, calc_crc);
            }
            
            if (calc_crc == rx_crc) {
                // Return ACK or Process
                if (msg_type == PKT_TYPE_CONFIG) {
                    if ((msg_len - 1) == sizeof(ConfigData_t)) {
                        ConfigData_t *cfg = (ConfigData_t*)msg_data;
                        current_seq = cfg->sequence;
                        PLL_Configure(cfg);
                        Protocol_SendAck(cfg->sequence, 0);
                    } else {
                        // Size mismatch for Config
                        uint8_t seq = 0;
                        if (msg_len > 1) seq = msg_data[0];
                        Protocol_SendAck(seq, 1); // Status 1: Format Error
                    }
                } else if (msg_type == PKT_TYPE_SETUP) {
                    if ((msg_len - 1) == sizeof(SetupData_t)) {
                        SetupData_t *setup = (SetupData_t*)msg_data;
                        g_pcb_revision = setup->pcb_revision;
                        g_setup_received = true;
                        Protocol_SendAck(setup->sequence, 0);
                    } else {
                        uint8_t seq = 0;
                        if (msg_len > 1) seq = msg_data[0];
                        Protocol_SendAck(seq, 1); // Status 1: Format Error
                    }
                } else if (msg_type == PKT_TYPE_GET_BUILD_ID) {
                    if ((msg_len - 1) == sizeof(GetBuildIdData_t)) {
                        GetBuildIdData_t *req = (GetBuildIdData_t*)msg_data;
                        Protocol_SendBuildId(req->sequence, g_build_id);
                    } else {
                        uint8_t seq = 0;
                        if (msg_len > 1) seq = msg_data[0];
                        Protocol_SendAck(seq, 1); // Status 1: Format Error
                    }
                }
            } else {
                // Checksum fail
                // Send NACK
                // Try to recover sequence?
                uint8_t seq = 0;
                if (msg_type == PKT_TYPE_CONFIG && msg_len > 1) seq = msg_data[0];
                Protocol_SendAck(seq, 3); // Status 3: CRC Error
            }
            
            state = STATE_SYNC1; 
            break;
    }
}

void Protocol_CheckTimeout(void)
{
    if (state != STATE_SYNC1)
    {
        if ((ms_ticks - last_byte_time) > 5)
        {
            // Timeout
            state = STATE_SYNC1;
            Protocol_SendAck(0, 2); 
        }
    }
}

static void SendPacket(uint8_t type, uint8_t *data, uint8_t len)
{
    // [SYNC1, SYNC2, LEN, TYPE, DATA..., CRC32]
    // LEN = 1 + len
    uint8_t total_len = 1 + len;
    
    UART_WriteByte(SYNC_BYTE_1);
    UART_WriteByte(SYNC_BYTE_2);
    UART_WriteByte(total_len);
    UART_WriteByte(type);
    UART_Write(data, len);
        uint8_t header[2] = {total_len, type};
    uint32_t crc = CRC_Calculate(header, 2, CRC_INITIAL_VALUE);
    crc = CRC_Calculate(data, len, crc);
    
    // Little Endian CRC send
    UART_WriteByte(crc & 0xFF);
    UART_WriteByte((crc >> 8) & 0xFF);
    UART_WriteByte((crc >> 16) & 0xFF);
    UART_WriteByte((crc >> 24) & 0xFF);
}

void Protocol_SendTelemetry(uint32_t pps_width, int32_t pll_error, uint32_t output_errors, uint8_t config_sequence, uint8_t frequency_mhz)
{
    TelemetryData_t telem;
    telem.pps_width = pps_width;
    telem.pll_error = pll_error;
    telem.output_errors = output_errors;
    telem.config_sequence = config_sequence;
    telem.frequency_mhz = frequency_mhz;
    SendPacket(PKT_TYPE_TELEM, (uint8_t*)&telem, sizeof(TelemetryData_t));
}

void Protocol_SendHeartbeat(uint8_t temp, uint16_t vcc_min, uint16_t vcc_max, uint16_t poe_min, uint16_t poe_max)
{
    HeartbeatPayload_t hb;
    hb.temp_c_half_deg = temp;
    hb.vcc_min_adc  = vcc_min;
    hb.vcc_max_adc  = vcc_max;
    hb.poe_min_adc  = poe_min;
    hb.poe_max_adc  = poe_max;

    SendPacket(PKT_TYPE_HEARTBEAT, (uint8_t*)&hb, sizeof(HeartbeatPayload_t));
}

void Protocol_SendAck(uint8_t sequence, uint8_t status)
{
    AckData_t ack;
    ack.sequence = sequence;
    ack.status = status;
    SendPacket(PKT_TYPE_ACK, (uint8_t*)&ack, sizeof(AckData_t));
}

void Protocol_SendBuildId(uint8_t sequence, uint64_t build_id)
{
    BuildIdData_t data;
    data.sequence = sequence;
    data.build_id = build_id;
    SendPacket(PKT_TYPE_BUILD_ID, (uint8_t*)&data, sizeof(BuildIdData_t));
}
