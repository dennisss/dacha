use crate::avr::interrupts::*;
use crate::avr::mutex::*;
use crate::avr::registers::*;

static USART1_TX_MUTEX: Mutex = Mutex::new();

pub struct USART1 {}

impl USART1 {
    // Initializes to asynchronous 8N1 at 9600 bits/second.
    pub fn init() {
        let baud = 9600;
        let f_osc = 16000000;
        // Based on Table 18-1 (first row)
        let ubbr: u32 = (f_osc / (16 * baud)) - 1;
        // assert_eq!(ubbr, 51);
        unsafe {
            // Set baud rate
            avr_write_volatile(UBRR1L, ubbr as u8);
            avr_write_volatile(UBRR1H, (ubbr >> 8) as u8);
            // Don't double the data rate.
            avr_write_volatile(UCSR1A, 0);
            // Enable transmitter
            avr_write_volatile(UCSR1B, 1 << 3);
            // Set frame format.
            // (only need to set UCSZn0:1)
            avr_write_volatile(UCSR1C, 0b11 << 1);
        }
    }

    #[cfg(target_arch = "avr")]
    #[inline(never)]
    pub fn send_blocking(data: &[u8]) {
        for byte in data.iter().cloned() {
            unsafe {
                // Wait for empty transmit buffer (UDREn is set)
                while (avr_read_volatile(UCSR1A) & (1 << 5)) == 0 {}

                // Put byte into the trasmit buffer.
                avr_write_volatile(UDR1, byte);
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn send_blocking(data: &[u8]) {
        // print!("{:?}", data);
    }

    #[cfg(target_arch = "avr")]
    #[inline(never)]
    pub async fn send(data: &[u8]) {
        let mut tx = USART1Transmission::start().await;
        tx.write(data).await;
        drop(tx);
    }
}

/// Represents a set of contiguous bytes send over the TX line.
pub struct USART1Transmission {
    lock: MutexLock<'static>,
    ctx: InterruptEnabledContext,
}

impl USART1Transmission {
    pub async fn start() -> Self {
        // At most one thread should be able to send over the wire at a time.
        let lock = USART1_TX_MUTEX.lock().await;
        // Enable USART1 Data Register Empty interrupt (UDRIEn bit).
        let ctx = InterruptEnabledContext::new(UCSR1B, 1 << 5);

        Self { lock, ctx }
    }

    pub async fn write(&mut self, data: &[u8]) {
        for byte in data.iter().cloned() {
            unsafe {
                // Wait for empty transmit buffer (UDREn is set)
                while (avr_read_volatile(UCSR1A) & (1 << 5)) == 0 {
                    InterruptEvent::USART1DataRegisterEmpty.to_future().await;
                }
                // Put byte into the trasmit buffer.
                avr_write_volatile(UDR1, byte);
            }
        }
    }
}
