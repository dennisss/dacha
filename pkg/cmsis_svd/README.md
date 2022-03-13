

Top level we will define the following data structures:

NOTE: All structs have a non-public 'hidden' field to prevent construction without using the unsafe new operator.

```
pub struct Peripherals {
    uart: uart::UART

    // Contains one field per defined peripheral.
    ...
} 

impl Peripherals {
    /// Only way to create a Peripherals object. This should be called exactly once by a program.
    /// Then peripherals should be shared through object ownership.
    pub unsafe fn new() -> Self { ... }
}

pub enum Interrupt {
    UART = 1,
    ...
}

```

Individual peripherals along with any other structs defined for them live in their own module:

```
pub mod uart {
    pub struct UART { ... }

    impl UART {
        const BASE_ADDRESS: u32 = 0xXXXXXXXX;
    }

    impl Deref for UART {
        type Target = UARTRegisters;
        ...
    }

    impl DerefMut for UART { ... }

    pub struct UARTRegisters {
        
        pub tasks_starttx: tasks_starttx::TASKS_STARTTX

        ...
    }

    pub mod tasks_starttx {
        pub struct TASKS_STARTTX {

        }
    }

}


#[repr(u32)]
pub enum TaskTrigger {
    Triggered = 1
}

```


Can't use a RawRegister for everything as we need separte 
