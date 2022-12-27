use common::errors::*;

const TRIANGLE_SIZE: usize = 4 * 4 * 3 + 2;

pub struct STL {
    pub binary: bool,
    pub triangles: Vec<Triangle>,
}

impl STL {
    // TODO: What about the color?
    // TODO: Support reading directly into a mesh without going into an STL
    // descriptor first.
    pub async fn read(path: &str) -> Result<Self> {
        // TODO: Use a buffered reader.
        let mut f = File::open(path).await?;

        // Every type of file should have at least 6 bytes in the file.
        // TODO: What if it is a text format that begins with a comment or
        // whitespace?
        let mut magic = [0u8; 6];
        f.read_exact(&mut magic).await?;
        f.seek(SeekFrom::Start(0)).await?;
        for c in magic.as_mut() {
            *c = (*c as char).to_ascii_lowercase();
        }

        if magic == b"solid" {
            Self::read_text(f, shader)
        } else {
            Self::read_binary(f, shader)
        }
    }

    async fn read_text(mut f: File, shader: Arc<Shader>) -> Result<Self> {
        Err(err_msg("STL Text format not supported"))
    }

    // TODO: Support reading directly into a mesh without the conversion to/from
    // STL Triangle.
    async fn read_binary(mut f: File, shader: Arc<Shader>) -> Result<Self> {
        let mut header = [0u8; 80];
        f.read_exact(&mut header).await?;

        let ntriangles = f.read_le_u32().await?;

        let mut triangles = vec![];
        triangles.reserve(ntriangles);

        let mut buf = [0u8; TRIANGLE_SIZE];
        for i in 0..ntriangles {
            f.read_exact(&mut buf);
            let t = Triangle::parse(&buf)?;
            if t.attribute_byte_count != 0 {
                return Err(err_msg("Unexpected facet attributes"));
            }
            triangles.push(t);
        }

        // TODO: Make this into a helper function implemented for all readable
        // streams.
        let current_pos = f.seek(SeekFrom::Current(0)).await?;
        if current_pos != f.metadata().await?.size {
            return Err(err_msg("Did not read until end of file"));
        }

        Ok(Self {
            binary: true,
            triangles,
        })
    }

    pub fn to_mesh(&self, shader: Arc<Shader>) -> Mesh {
        let mut vertices = vec![];
        vertices.reserve(3 * self.triangles.len());
        let mut faces = vec![];
        faces.reserve(self.triangles.len());
        let mut normals = vec![];
        normals.reserve(self.triangles.len());

        for t in &self.triangles {
            let i = vertices.len();
            faces.push([i, i + 1, i + 2]);
            normals.push(t.normal);

            for point in &t.points {
                vertices.push(point.clone());
            }
        }

        Mesh::from(&vertices, &[], &faces, &normals, shader)
    }
}

// TODO: We should we able to automatically implement this parser
pub struct Triangle {
    normal: Vector3f,
    points: [Vector3f; 3],
    attribute_byte_count: u16,
}

impl Triangle {
    fn parse(mut input: &[u8]) -> Result<Self> {
        let normal = parse_next!(input, parse_vector3f);
        let point1 = parse_next!(input, parse_vector3f);
        let point2 = parse_next!(input, parse_vector3f);
        let point3 = parse_next!(input, parse_vector3f);

        let attribute_byte_count = parse_le_u16();

        if input.len() != 0 {
            return Err(err_msg("Not parsed till completion"));
        }

        Ok(Self {
            normal,
            points: [point1, point2, point3],
            attribute_byte_count,
        })
    }
}

pub fn parse_le_u16(input: &[u8]) -> Result<(u16, &[u8])> {
    if input.len() < 2 {
        return Err(err_msg("Input too short"));
    }

    Ok((u16::from_le_bytes(*array_ref![input, 0, 2]), &input[2..]))
}

pub fn parse_le_f32(input: &[u8]) -> Result<f32> {
    if input.len() < 4 {
        return Err(err_msg("Input too short"));
    }

    Ok((f32::from_le_bytes(*array_ref![input, 0, 4]), &input[4..]))
}

pub fn parse_vector3f(mut input: &[u8]) -> Result<(Vector3f, &[u8])> {
    let x = parse_next!(input, parse_le_f32);
    let y = parse_next!(input, parse_le_f32);
    let z = parse_next!(input, parse_le_f32);
    Ok((Vector3f::from_slice(x, y, z), input))
}
