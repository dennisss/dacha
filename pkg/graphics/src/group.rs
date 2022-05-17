use crate::drawable::{Drawable, Primitive};
use crate::transform::{AsMatrix, Camera, Transform};

/// Many shapes grouped together and drawn/transformed together
#[derive(Default)]
pub struct Group {
    primitive: Primitive,
    objects: Vec<Box<dyn Drawable>>,
}

impl_deref!(Group::primitive as Primitive);

impl Drawable for Group {
    fn draw(&self, camera: &Camera, prev: &Transform) {
        let t = prev.apply(self.primitive.transform());
        for object in self.objects.iter() {
            object.draw(camera, &t);
        }
    }
}

impl Group {
    pub fn add_object(&mut self, object: Box<dyn Drawable>) {
        self.objects.push(object);
    }

    pub fn clear(&mut self) {
        self.objects.clear();
    }
}

/*
void Group::removeObject(Drawable *o){
    for(int i = 0; i < objects.size(); i++){
        if(objects[i] == o){
            objects.erase(objects.begin() + i);
            return;
        }
    }
}

*/
