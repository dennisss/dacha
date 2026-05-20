#include "stm32f4xx.h"
#include "drivers.h"
#include "protocol.h"

// Baud Calculation for 96MHz PCLK2, 115200 Baud, OVER8=0
// DIV = 96000000 / (16 * 115200) = 52.0833
// Mantissa = 52 (0x34)
// Fraction = 0.0833 * 16 = 1.33 -> 1
// BRR = 0x341

void UART_Init(void)
{
    // Enable USART1 Clock (RCC done in RCC_Init, but safe to assume enabled)
    
    // Disable UART
    USART1->CR1 = 0;
    
    // Set Baud Rate (115200)
    USART1->BRR = 0x341;
    
    // Enable RXNE Interrupt
    USART1->CR1 |= USART_CR1_RXNEIE;
    
    // Enable TX, RX, UE
    USART1->CR1 |= (USART_CR1_TE | USART_CR1_RE | USART_CR1_UE);
    
    // Enable Interrupt in NVIC
    NVIC_EnableIRQ(USART1_IRQn);
}

void UART_WriteByte(uint8_t byte)
{
    while (!(USART1->SR & USART_SR_TXE)); // Wait for TXE
    USART1->DR = byte;
}

void UART_Write(uint8_t *data, uint32_t len)
{
    for (uint32_t i = 0; i < len; i++)
    {
        UART_WriteByte(data[i]);
    }
}

// ISR
void USART1_IRQHandler(void)
{
    if (USART1->SR & USART_SR_RXNE)
    {
        uint8_t data = USART1->DR;
        Protocol_ProcessByte(data);
    }
    
    if (USART1->SR & (USART_SR_ORE | USART_SR_NE | USART_SR_FE | USART_SR_PE))
    {
        volatile uint32_t tmpreg = USART1->SR;
        tmpreg = USART1->DR;
        (void)tmpreg;
    }
}
