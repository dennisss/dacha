use math::geometry::half_edge::HalfEdgeStruct;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(crate) struct FaceLabel {
    /// True if present on the copper gerber layer.
    pub dark: bool,

    /// True if the face is inside of the outer most edge cut of the board.
    pub inbounds: bool,
}

impl FaceLabel {
    pub fn dark() -> Self {
        Self { dark: true, inbounds: false }
    }

    pub fn inbounds() -> Self {
        Self { dark: false, inbounds: true }
    }
}

impl math::geometry::half_edge::FaceLabel for FaceLabel {
    fn union(&self, other: &Self) -> Self {
        Self {
            dark: self.dark || other.dark,
            inbounds: self.inbounds || other.inbounds
        }
    }
}

pub fn objects_to_faces<'a, I: Iterator<Item = &'a gerber::GraphicsObject>>(
    objects: I,
    max_error: f32
) -> HalfEdgeStruct<FaceLabel> {
    let mut half_edges = HalfEdgeStruct::<FaceLabel>::new();

    for obj in objects {
        for path in &obj.paths {
            if let gerber::FillMode::Dark = path.fill {
                //
            } else {
                // TODO:
                println!("Non dark");
                continue;
            }

            let (vertices, path_starts) = path.path.linearize(max_error);
            for i in 0..(path_starts.len() - 1) {
                let start_i = path_starts[i];
                let end_i = path_starts[i + 1];
                half_edges.add_face(FaceLabel::dark(), vertices[start_i..end_i].iter().cloned());
            }
        }
    }

    half_edges.repair();
    half_edges.merge_faces();
    half_edges
}