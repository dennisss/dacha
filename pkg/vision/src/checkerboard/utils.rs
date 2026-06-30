use std::marker::PhantomData;

pub struct Image1cRef<T: AsRef<[V]>, V: Copy> {
    data: T,
    width: usize,
    height: usize,
    t: PhantomData<V>,
}

impl<T: AsRef<[V]>, V: Copy> Image1cRef<T, V> {

    pub fn new(data: T, height: usize, width: usize) -> Self {
        Self {
            data,
            width, 
            height,
            t: PhantomData
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn get(&self, y: usize, x: usize) -> V {
        self.data.as_ref()[y * self.width + x]
    }

    pub fn visit_neighbors<F: FnMut(usize, usize, V)>(&self, y: usize, x: usize, mut f: F) {
        let y = y as isize;
        let x = x as isize;

        for y_step in -1..2 {
            let y_i = y + y_step;
            if y_i < 0 || y_i >= (self.height as isize) {
                continue;
            }

            for x_step in -1..2 {
                let x_i = x + x_step;
                if x_i < 0 || x_i >= (self.width as isize) {
                    continue;
                }

                // Don't count ourselves.
                if x_i == (x as isize) && y_i == (y as isize) {
                    continue;
                }

                let v = self.get(y_i as usize, x_i as usize);
                f(y_i as usize, x_i as usize, v);
            }
        }
    }
}

impl<T: AsRef<[V]> + AsMut<[V]>, V: Copy> Image1cRef<T, V> {
    pub fn set(&mut self, y: usize, x: usize, v: V) {
        self.data.as_mut()[y * self.width + x] = v;
    }
}

