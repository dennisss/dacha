
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub fn compute_jtj(jacobian: &[f64], residuals: &[f64], rows: usize, cols: usize, h: &mut [f64], g: &mut [f64]) {
    unsafe {
        bindings::compute_jtj(
            jacobian.as_ptr(),
            residuals.as_ptr(),
            rows as i32,
            cols as i32,
            h.as_mut_ptr(),
            g.as_mut_ptr()
        );
    }
}

pub fn solve_sparse_ldlt(a: &[f64], b: &[f64], x: &mut [f64]) -> bool {
    let success = unsafe {
        bindings::solve_sparse_ldlt(
            a.as_ptr(),
            b.as_ptr(),
            x.len() as std::os::raw::c_int,
            x.as_mut_ptr()
        )
    };

    success == 1
}
