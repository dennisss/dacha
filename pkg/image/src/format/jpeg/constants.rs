pub const BLOCK_DIM: usize = 8;
pub const BLOCK_SIZE: usize = 64;

// TODO: We need to validate that this actually applies.
// TODO: In sequential and lossless modes, this can be up to 255
pub const MAX_NUM_COMPONENTS: usize = 4;

pub const MAX_DC_TABLES: usize = 4;
pub const MAX_AC_TABLES: usize = 4;
pub const MAX_QUANT_TABLES: usize = 4;
