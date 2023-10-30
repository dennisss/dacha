# Numerical Computation Library

f = y = 'm x1 + x2'

y = a + b

a = m * x1

b = x2

df/dy = 1

df/da = 


.........



Eventual goal is to have a set of tracked streams:


- Find all places those streams are used:
    - So assume we have 'x'



want dy/dx1, dy/dx2

InputOp for x1 : out1 = x1, dout1 / dx1 = 1

Need ConstantOp
    - 

MulOp 'm x1' : out2 = 'm out1'
    - dout2 / x1 = 

When we save a graph, 


'd(uv) / dx = u (dv/dx) + v du / dx'


first compute 



Chain rule:

- 'dy/dx = (dy/du) * (du/dx)'

- (dOutput / dInput) = (dOutput / dU) * (dU / dInput)





Want to allow a 





Back propagation:

- Inputs: 'x',
- Outputs: 'y'

'dL/di'


i = A1( W1 * x )
j = A2( W2 * i )

L = (j(i) - y)^2

dL/di = (dL / dj) ( dj / di )

dL/dj = (2 * (j - y)) * ( dj / di )


- Assume 



We have some operation graph representing unevaluated operationswe can perform

- 

For gradients,
- Want to know gradient of 'output_tensor' relative at a set of input tensors.
- Constant operations 
- Except must simplify the graph so that add/mul by zero is pruned.



Need to support numerical computation graphs 

Linear regression



Layer definition:

- A layer/module is a trait:
	- Has a 'State' type.
		- Special because we want to be able to initialize it and evolve it.
	- Has an 'Input' type.
	- Has an 'Output' type.
- Should be creatable and serializable from a 'HParams' object


`state`, `inputs`

Goals:
- Given an opaque function
    - Want to be able `evaluate()` it.
    - Want to be able to probe inputs to the function
        - Some inputs may have initializers or be marked as trainable.
    - Want to be able to probe the outputs

train(loss_fn, data) {
    Initialize all 'variables'
    Generate gradient outputs
    Run loss_fn with 'variables' + data => loss + gradients
    Generate new variables based on gradients*learning_rate
    Optionally save everything.
    Loop
}

Training process:
- Create a function
- Initialize 


- In a module,
    - Must be able to register state variables.
    - Must be able to double check 

What is training:
    - State: `map<OutputKey, Tensor>`
    - MOdules have some of 
    - Feed inputs and 



