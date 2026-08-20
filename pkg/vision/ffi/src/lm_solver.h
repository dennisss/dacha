#ifndef LM_SOLVER_H
#define LM_SOLVER_H

#ifdef __cplusplus
extern "C" {
#endif

void compute_jtj(
    const double* J_ptr,
    const double* f_ptr,
    int rows,
    int cols,
    double* H_out,
    double* g_out
);

int solve_sparse_ldlt(
    const double* a_ptr,
    const double* b_ptr,
    int cols,
    double* x_ptr
);

#ifdef __cplusplus
}
#endif

#endif // LM_SOLVER_H