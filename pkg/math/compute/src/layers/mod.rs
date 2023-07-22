use crate::{DataType, Output};

/////////
/// Activation functions
/////////

/// Converts a linear number to a probability between 0 and 1 (used for creating
/// a logistic regression output).
pub fn sigmoid(x: Output) -> Output {
    1 / (1 + (-x).exp())
}

/// Converts a vector of numbers to a vector of probabilities.
/// For constructing a multi-class predictor where classes don't overlap.
pub fn softmax(x: Output, axis: isize) -> Output {
    let ex = x.exp();
    &ex / ex.sum(&[-1], true)
}

/////////
/// Loss functions
/////////

// cwise_sum

/// Computes the mean squared error between two similarly shaped tensors.
/// Outputs a scalar error.
///
/// Inputs should be of shape [N]
pub fn mean_squared_error(y: Output, y2: Output) -> Output {
    // TODO: Start a new subgroup here.
    // TODO: Assert 'y' and 'y2' are the same shape and are 1D

    let e = y - y2;

    // TODO: Reduce sum this.
    let e = &e * &e;

    // TODO: Use the last dimension size
    let scale = 1 / e.size().cast(DataType::Float32);

    // TODO: Sum up all axes?
    (scale * e).sum(&[-1], false)
}

/// Inputs should be of shape [N]. Each element should be a 0-1 float
/// probability.
pub fn log_loss(y_predict: &Output, y_expect: &Output) -> Output {
    let e = (-y_expect * y_predict.ln()) - ((1 - y_expect) * (1 - y_predict).ln());

    // TODO: Use the last dimension size
    let scale = 1 / e.size().cast(DataType::Float32);

    e.sum(&[-1], false) / scale
}
