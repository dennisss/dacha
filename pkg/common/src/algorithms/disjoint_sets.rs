pub struct DisjointSets {
    elements: Vec<ElementData>,
}

#[derive(Clone)]
struct ElementData {
    parent: usize,
    min: usize,
    rank: usize,
}

impl Default for ElementData {
    fn default() -> Self {
        ElementData {
            parent: 0,
            min: 0,
            rank: 0,
        }
    }
}

impl DisjointSets {
    /// Creates a new collection of disconnected elements
    pub fn new(n: usize) -> Self {
        let mut sets = DisjointSets {
            elements: Vec::new(),
        };

        sets.elements.resize(n, ElementData::default());
        sets.clear();

        sets
    }

    /// Resets the datastructure such that all elements are disjoint
    pub fn clear(&mut self) {
        for i in 0..self.elements.len() {
            self.make_set(i);
        }
    }

    /// (Re)Initializes an element to be in its own set of size 1
    fn make_set(&mut self, x: usize) {
        let e_x = &mut self.elements[x];
        e_x.rank = 0;
        e_x.parent = x;
        e_x.min = x;
    }

    /// Finds the unique number identifying a set
    ///
    /// Returns a number representing the index of the root element of the set
    /// containing x
    ///
    /// # Arguments
    /// * `x` - the index of one element of the set
    pub fn find_set(&mut self, x: usize) -> usize {
        let p_old = self.elements[x].parent;
        if p_old != x {
            self.elements[x].parent = self.find_set(p_old);
        }

        self.elements[x].parent
    }

    /// Like findSet, but uses the index of the smallest element in the set to
    /// identify it
    pub fn find_set_min(&mut self, mut x: usize) -> usize {
        x = self.find_set(x);

        let e_x = &mut self.elements[x];
        return e_x.min;
    }

    /// Joins the sets consisting of the two given elements into one set
    pub fn union_sets(&mut self, mut x: usize, mut y: usize) {
        // Find root labels
        x = self.find_set(x);
        y = self.find_set(y);

        // if x and y are already in the same set (i.e., have the same root or
        // representative)
        if x == y {
            return;
        }

        let e = &mut self.elements;

        // x and y are not in same set, so we merge them
        if e[x].rank < e[y].rank {
            e[x].parent = y;
        } else if e[x].rank > e[y].rank {
            e[y].parent = x;
        } else {
            e[y].parent = x;
            e[x].rank = e[x].rank + 1;
        }

        // Update the min in the root of the new set
        if e[x].min < e[y].min {
            e[y].min = e[x].min;
        } else {
            e[x].min = e[y].min;
        }
    }
}
