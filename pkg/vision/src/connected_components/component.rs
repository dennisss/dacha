
#[derive(Debug, Clone)]
pub struct ComponentData {
    pub min_x: u16,
    pub max_x: u16,
    pub min_y: u16,
    pub max_y: u16,

    pub area: u32,
    pub mass: u64,
    pub moment_x: u64,
    pub moment_y: u64,
}

impl ComponentData {
    pub fn empty() -> Self {
        Self {
            min_x: std::u16::MAX,
            min_y: std::u16::MAX,
            max_x: 0,
            max_y: 0,
            area: 0,
            mass: 0,
            moment_x: 0,
            moment_y: 0
        }
    }

    pub fn add_pixel(&mut self, x: usize, y: usize, intensity: u64) {
        self.min_x = self.min_x.min(x as u16);
        self.max_x = self.max_x.max(x as u16);
        self.min_y = self.min_y.min(y as u16);
        self.max_y = self.max_y.max(y as u16);

        self.area += 1;
        self.mass += intensity;
        self.moment_x += (x as u64) * intensity;
        self.moment_y += (y as u64) * intensity;
    }

    pub fn start_pixel_row(&mut self, x: usize, y: usize) {
        self.area = 1; // Just so that other logic believes this isn't an empty data structure.
        self.min_x = x as u16;
        self.max_x = x as u16;
        self.min_y = y as u16;
        self.max_y = y as u16;
    }

    pub fn add_row_pixel(&mut self, x: usize, y: usize, intensity: u64) {
        self.max_x = x as u16;
        self.mass += intensity;
        self.moment_x += (x as u64) * intensity;
    }

    pub fn finish_pixel_row(&mut self) {
        self.area = (self.max_x - self.min_x) as u32;
        self.moment_y = self.mass * (self.min_y as u64) ;
    }

    pub fn add(&mut self, other: &Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.max_x = self.max_x.max(other.max_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_y = self.max_y.max(other.max_y);

        self.area += other.area;
        self.mass += other.mass;
        self.moment_x += other.moment_x;
        self.moment_y += other.moment_y;
    }
}

