

/// Based on the SAUF paper.
#[derive(Default)]
pub struct DisjointSets {
    // TODO: Make private eventually.
    pub(super) parents: Vec<u32>,
}

impl DisjointSets {
    pub fn reserve(&mut self, num: usize) {
        self.parents.reserve(num);
    }

    pub fn find_root(&self, i: usize) -> usize {
        assert!(i < self.parents.len());

        // We assume the 'parents' is internally well formed and the above
        // assertion validates 'i' as a valid entry point into that array.
        unsafe { 
            let mut root = i;
            while (*self.parents.get_unchecked(root) as usize) < root {
                root = (*self.parents.get_unchecked(root) as usize);
            }

            root
        }

    }

    pub fn is_root(&self, i: usize) -> bool {
        self.parents[i] == (i as u32)
    }

    // NOTE: Safety of this assumes that all callers have validates the given indexes are correct.
    fn set_root(&mut self, mut i: usize, root: usize) {
        unsafe {
            while (*self.parents.get_unchecked(i) as usize) < i {
                let j = *self.parents.get_unchecked(i) as usize;
                *self.parents.get_unchecked_mut(i) = root as u32;
                i = j;
            }

            *self.parents.get_unchecked_mut(i) = root as u32;
        }
    }

    pub fn find(&mut self, i: usize) -> usize {
        let root = self.find_root(i);
        self.set_root(i, root);
        root
    }

    pub fn union(&mut self, i: usize, j: usize) -> usize {
        if i == j {
            // TODO: Just run the below code.
            return self.find_root(i);
        }

        let root = core::cmp::min(
            self.find_root(i),
            self.find_root(j)
        );

        self.set_root(i, root);
        self.set_root(j, root);

        root
    }

    pub fn clear(&mut self) {
        self.parents.clear();
    }
    
    pub fn new_set(&mut self) -> usize {
        let i = self.parents.len();
        self.parents.push(i as u32);
        i
    }

    pub fn flatten(&mut self) {
        for i in 0..self.parents.len() {
            let p = self.parents[i] as usize;
            self.parents[i] = self.parents[p];
        }
    }

    pub fn flatten_and_relabel(&mut self) -> usize {
        let mut k = 0;
        for i in 0..self.parents.len() {
            if self.parents[i] < (i as u32) {
                let p = self.parents[i] as usize;
                self.parents[i] = self.parents[p];
            } else {
                self.parents[i] = k;
                k += 1;
            }
        }

        k as usize
    }
}