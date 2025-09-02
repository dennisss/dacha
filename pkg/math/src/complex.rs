use core::ops::*;

#[derive(Clone, Copy, PartialEq)]
pub struct Complex {
    real: f32,
    imag: f32,
}

impl Complex {
    pub fn new(real: f32, imag: f32) -> Self {
        Self { real, imag }
    }

    pub fn real(&self) -> f32 {
        self.real
    }

    pub fn imag(&self) -> f32 {
        self.imag
    }

    /// TODO: Return an f32
    pub fn abs(&self) -> Self {
        Self::new((self.real*self.real + self.imag*self.imag).sqrt(), 0.0)
    }

    pub fn inv(&self) -> Self {
        let a = self.real;
        let b = self.imag;

        let c = a*a + b*b;
        Self::new(a / c, -b / c)
    }

    pub fn try_inv(&self, tolerance: f32) -> Option<Self> {
        let a = self.real;
        let b = self.imag;

        let c = a*a + b*b;
        if c < tolerance {
            return None;
        }

        Some(Self::new(a / c, -b / c))
    }

    pub fn conjugate(&self) -> Self {
        Self::new(self.real, -self.imag)
    }

}

impl Add for Complex {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.real + rhs.real, self.imag + rhs.imag)
    }
}

impl Sub for Complex {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.real - rhs.real, self.imag - rhs.imag)
    }
}

impl Mul for Complex {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let a = self.real;
        let b = self.imag;
        let c = rhs.real;
        let d = rhs.imag;
        Self::new(a*c - b*d, a*d + b*c)
    }
}
