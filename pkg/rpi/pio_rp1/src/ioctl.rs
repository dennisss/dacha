// Code derived from the kernel definitions.

use sys::{iowr, ior, iow, ion};

use crate::bindings::*;

const PIO_IOC_MAGIC: u8 = 102;

iow!(pio_ioc_sm_config_xfer, PIO_IOC_MAGIC, 0, rp1_pio_sm_config_xfer_args);
iow!(pio_ioc_sm_xfer_data, PIO_IOC_MAGIC, 1, rp1_pio_sm_xfer_data_args);
iow!(pio_ioc_sm_xfer_data32, PIO_IOC_MAGIC, 2, rp1_pio_sm_xfer_data32_args);
iow!(pio_ioc_sm_config_xfer32, PIO_IOC_MAGIC, 3, rp1_pio_sm_config_xfer32_args);

iow!(pio_ioc_read_hw, PIO_IOC_MAGIC, 8, rp1_access_hw_args);
iow!(pio_ioc_write_hw, PIO_IOC_MAGIC, 9, rp1_access_hw_args);

iow!(pio_ioc_can_add_program, PIO_IOC_MAGIC, 10, rp1_pio_add_program_args);
iow!(pio_ioc_add_program, PIO_IOC_MAGIC, 11, rp1_pio_add_program_args);
iow!(pio_ioc_remove_program, PIO_IOC_MAGIC, 12, rp1_pio_remove_program_args);
ion!(pio_ioc_clear_instr_mem, PIO_IOC_MAGIC, 13);

iow!(pio_ioc_sm_claim, PIO_IOC_MAGIC, 20, rp1_pio_sm_claim_args);
iow!(pio_ioc_sm_unclaim, PIO_IOC_MAGIC, 21, rp1_pio_sm_claim_args);
iow!(pio_ioc_sm_is_claimed, PIO_IOC_MAGIC, 22, rp1_pio_sm_claim_args);

iow!(pio_ioc_sm_init, PIO_IOC_MAGIC, 30, rp1_pio_sm_init_args);
iow!(pio_ioc_sm_set_config, PIO_IOC_MAGIC, 31, rp1_pio_sm_set_config_args);
iow!(pio_ioc_sm_exec, PIO_IOC_MAGIC, 32, rp1_pio_sm_exec_args);
iow!(pio_ioc_sm_clear_fifos, PIO_IOC_MAGIC, 33, rp1_pio_sm_clear_fifos_args);
iow!(pio_ioc_sm_set_clkdiv, PIO_IOC_MAGIC, 34, rp1_pio_sm_set_clkdiv_args);
iow!(pio_ioc_sm_set_pins, PIO_IOC_MAGIC, 35, rp1_pio_sm_set_pins_args);
iow!(pio_ioc_sm_set_pindirs, PIO_IOC_MAGIC, 36, rp1_pio_sm_set_pindirs_args);
iow!(pio_ioc_sm_set_enabled, PIO_IOC_MAGIC, 37, rp1_pio_sm_set_enabled_args);
iow!(pio_ioc_sm_restart, PIO_IOC_MAGIC, 38, rp1_pio_sm_restart_args);
iow!(pio_ioc_sm_clkdiv_restart, PIO_IOC_MAGIC, 39, rp1_pio_sm_restart_args);
iow!(pio_ioc_sm_enable_sync, PIO_IOC_MAGIC, 40, rp1_pio_sm_enable_sync_args);
iow!(pio_ioc_sm_put, PIO_IOC_MAGIC, 41, rp1_pio_sm_put_args);
iowr!(pio_ioc_sm_get, PIO_IOC_MAGIC, 42, rp1_pio_sm_get_args);
iow!(pio_ioc_sm_set_dmactrl, PIO_IOC_MAGIC, 43, rp1_pio_sm_set_dmactrl_args);
iow!(pio_ioc_sm_fifo_state, PIO_IOC_MAGIC, 44, rp1_pio_sm_fifo_state_args);
iow!(pio_ioc_sm_drain_tx, PIO_IOC_MAGIC, 45, rp1_pio_sm_clear_fifos_args);

iow!(pio_ioc_gpio_init, PIO_IOC_MAGIC, 50, rp1_gpio_init_args);
iow!(pio_ioc_gpio_set_function, PIO_IOC_MAGIC, 51, rp1_gpio_set_function_args);
iow!(pio_ioc_gpio_set_pulls, PIO_IOC_MAGIC, 52, rp1_gpio_set_pulls_args);
iow!(pio_ioc_gpio_set_outover, PIO_IOC_MAGIC, 53, rp1_gpio_set_args);
iow!(pio_ioc_gpio_set_inover, PIO_IOC_MAGIC, 54, rp1_gpio_set_args);
iow!(pio_ioc_gpio_set_oeover, PIO_IOC_MAGIC, 55, rp1_gpio_set_args);
iow!(pio_ioc_gpio_set_input_enabled, PIO_IOC_MAGIC, 56, rp1_gpio_set_args);
iow!(pio_ioc_gpio_set_drive_strength, PIO_IOC_MAGIC, 57, rp1_gpio_set_args);

