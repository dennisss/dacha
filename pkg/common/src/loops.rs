use crate::errors::*;

pub enum Loop {
    Continue,
    Break,
}

pub fn bounded_loop<F: FnMut() -> Result<Loop>>(max_iters: usize, mut f: F) -> Result<()> {
    for _ in 0..max_iters {
        match f()? {
            Loop::Break => {
                return Ok(());
            }
            Loop::Continue => {
                continue;
            }
        }
    }

    Err(err_msg("Exceeded max iterations"))
}
