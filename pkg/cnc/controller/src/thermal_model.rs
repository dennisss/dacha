

/// Maximum size of the step we are allowed to take in a simulation (in seconds).
const MAX_SIMULATION_TIMESTEP: f32 = 0.025;

#[derive(Default, Clone)]
pub struct ThermalFEM {
    /// Temperature of each element in the system.
    pub elements: Vec<f32>,

    next_elements: Vec<f32>,

    // deriv: Vec<f32>,

    // elements_temp: Vec<f32>,
    // k1: Vec<f32>,
    // k2: Vec<f32>,
    // k3: Vec<f32>,
    // k4: Vec<f32>,

    pub model: ThermalModel,
}

#[derive(Default, Clone)]
pub struct ThermalModel {
    // num_elements?

    /// from_element_index, to_element_index, coefficient
    pub relations: Vec<(usize, usize, f32)>,

    pub sources: Vec<(usize, f32)>,
}

impl ThermalFEM {

    /// Advances forward the state of the simulation by 'dt' amount of time.
    pub fn step(&mut self, dt: f32) {
        let mut t = 0.0;

        while t < dt {
            let next_t = (t + dt).min(t + MAX_SIMULATION_TIMESTEP);
            self.single_step(next_t - t);
            t = next_t;
        }
    }

    fn single_step(&mut self, dt: f32) {
        /*
        self.model.compute_derivatives(&self.elements, &mut self.k1);

        let dt2 = (dt / 2.0);
        for i in 0..self.elements.len() {
            self.elements_temp[i] = self.elements[i] + dt2 * self.k1[i];
        }
        self.model.compute_derivatives(&self.elements_temp, &mut self.k2);

        for i in 0..self.elements.len() {
            self.elements_temp[i] = self.elements[i] + dt2 * self.k2[i];
        }
        self.model.compute_derivatives(&self.elements_temp, &mut self.k3);

        for i in 0..self.elements.len() {
            self.elements_temp[i] = self.elements[i] + dt * self.k3[i];
        }
        self.model.compute_derivatives(&self.elements_temp, &mut self.k4);
        
        let dt6 = (dt / 6.0);
        for i in 0..self.elements.len() {
            self.elements[i] += dt6 * (self.k1[i] + 2.0 * self.k2[i] + 2.0 * self.k3[i] + self.k4[i]);
        }
        */

        for i in 0..self.elements.len() {
            self.next_elements[i] = self.elements[i];
        }

        for (from_i, to_i, coeff) in self.model.relations.iter().cloned() {
            let scale = self.elements[from_i] - self.elements[to_i];
            self.next_elements[to_i] += coeff * scale * dt;
        }

        for (source_target, coeff) in self.model.sources.iter().cloned() {
            self.next_elements[source_target] += coeff * dt; 
        }

        core::mem::swap(&mut self.elements, &mut self.next_elements);
    }

    pub fn add_element(&mut self, initial_temp: f32) -> usize {
        let n = self.elements.len();
        self.elements.push(initial_temp);
        self.next_elements.push(initial_temp);
        // self.deriv.push(0.0);

        // self.elements_temp.push(0.0);
        // self.k1.push(0.0);
        // self.k2.push(0.0);
        // self.k3.push(0.0);
        // self.k4.push(0.0);

        n
    }

    pub fn clear_sources(&mut self) {
        self.model.sources.clear();
    }

}

impl ThermalModel {
    fn compute_derivatives(&self, elements: &[f32], out: &mut [f32]) {
        for v in out.iter_mut() {
            *v = 0.0;
        }

        for (from_i, to_i, coeff) in self.relations.iter().cloned() {
            let scale = elements[from_i] - elements[to_i];
            out[to_i] += coeff * scale;
        }

        for (source_target, coeff) in self.sources.iter().cloned() {
            out[source_target] += coeff; 
        }
    }

}