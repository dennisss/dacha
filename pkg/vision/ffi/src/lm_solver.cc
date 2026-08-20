#include <Eigen/Dense>
#include <Eigen/Sparse>
#include <Eigen/SparseCholesky>

extern "C" {

    // Computes J^T J and J^T f
    void compute_jtj(
        const double* J_ptr,     // Input: Jacobian (rows x cols, Row-Major)
        const double* f_ptr,     // Input: Residuals (rows)
        int rows, 
        int cols,
        double* H_out,           // Output: Hessian (cols x cols, Row-Major)
        double* g_out            // Output: Gradient (cols)
    ) {
        using ConstMatrixMap = Eigen::Map<const Eigen::Matrix<double, Eigen::Dynamic, Eigen::Dynamic, Eigen::RowMajor>>;
        using ConstVectorMap = Eigen::Map<const Eigen::VectorXd>;
        ConstMatrixMap J(J_ptr, rows, cols);
        ConstVectorMap f(f_ptr, rows);

        // TODO: I know which entries are non-zero when calculating the gradients
        // and the mapping is always consistent across steps so this could be optimized.
        using SpMat = Eigen::SparseMatrix<double, Eigen::RowMajor>;
        SpMat J_sparse = J.sparseView();

        using MutMatrixMap = Eigen::Map<Eigen::Matrix<double, Eigen::Dynamic, Eigen::Dynamic, Eigen::RowMajor>>;
        using MutVectorMap = Eigen::Map<Eigen::VectorXd>;
        MutMatrixMap H(H_out, cols, cols);
        MutVectorMap g(g_out, cols);

        // Compute and write directly to Rust memory
        H = J_sparse.transpose() * J_sparse;
        g = J_sparse.transpose() * f;
    }

    int solve_sparse_ldlt(
        const double* a_ptr,
        const double* b_ptr,
        int cols,
        double* x_ptr
    ) {
        using ConstMatrixMap = Eigen::Map<const Eigen::Matrix<double, Eigen::Dynamic, Eigen::Dynamic, Eigen::RowMajor>>;
        using ConstVectorMap = Eigen::Map<const Eigen::VectorXd>;
        ConstMatrixMap A(a_ptr, cols, cols);
        ConstVectorMap b(b_ptr, cols);

        using SpMat = Eigen::SparseMatrix<double, Eigen::RowMajor>;
        SpMat A_sparse = A.sparseView();

        Eigen::SimplicialLDLT<SpMat> solver;
        solver.compute(A_sparse);
        
        if (solver.info() != Eigen::Success) {
            return 0; // Failure
        }

        Eigen::Map<Eigen::VectorXd> x(x_ptr, cols);
        x = solver.solve(b);
        
        return 1; // Success
    }
}