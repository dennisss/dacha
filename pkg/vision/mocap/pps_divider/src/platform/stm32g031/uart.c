#include "stm32g0xx.h"
#include "drivers.h"
#include "protocol.h"

void UART_Init(void)
{
    // Enable USART1 Clock (Done in RCC)
    
    // Disable USART
    USART1->CR1 &= ~USART_CR1_UE;
    
    // Baud Rate = 115,200
    // PCLK = 64 MHz
    // USARTDIV = 64000000 / 115200 = 555.55 -> 556
    USART1->BRR = 556;
    
    // Enable USART (MUST be done before TE/RE)
    USART1->CR1 |= USART_CR1_UE;
    
    // Enable TX, RX
    USART1->CR1 |= USART_CR1_TE | USART_CR1_RE;
    
    // Enable RXNE Interrupt
    USART1->CR1 |= USART_CR1_RXNEIE_RXFNEIE;
    
    // Enable Interrupt in NVIC
    NVIC_EnableIRQ(USART1_IRQn);
    
    // Wait for TEACK and REACK
    while((USART1->ISR & USART_ISR_TEACK) == 0);
    while((USART1->ISR & USART_ISR_REACK) == 0);
}

void UART_Write(uint8_t *data, uint32_t len)
{
    for (uint32_t i = 0; i < len; i++)
    {
        while (!(USART1->ISR & USART_ISR_TXE_TXFNF));
        USART1->TDR = data[i];
    }
    while (!(USART1->ISR & USART_ISR_TC));
}

void UART_WriteByte(uint8_t byte)
{
    UART_Write(&byte, 1);
}

// ISR 
void USART1_IRQHandler(void)
{
    if (USART1->ISR & USART_ISR_RXNE_RXFNE)
    {
        uint8_t data = USART1->RDR;
        Protocol_ProcessByte(data);
    }
    
    if (USART1->ISR & (USART_ISR_ORE | USART_ISR_NE | USART_ISR_FE | USART_ISR_PE))
    {
        USART1->ICR = (USART_ICR_ORECF | USART_ICR_NECF | USART_ICR_FECF | USART_ICR_PECF);
    }
}
