
/// TODO: Do the modelview and proj merging on the CPU.
pub const VERTEX_SHADER: &'static str = r#"
#version 330

uniform mat4 u_proj;
uniform mat4 u_modelview;

in vec3 v_position;
in vec3 v_color;
in vec2 v_tex_coord;

out vec2 f_tex_coord;
out vec4 f_color;

void main() {
    f_color = vec4(v_color, 1.0);
    f_tex_coord = v_tex_coord;
	gl_Position = u_proj * u_modelview * vec4(v_position, 1.0);
}
"#;

pub const SPHERE_FRAGMENT_SHADER: &'static str = r#"
#version 330

in vec2 f_tex_coord;
in vec4 f_color;

out vec4 frag_color;

void main() {
	frag_color = f_color;
}
"#;


/// Fragment shader that does the following:
/// - Input a texture to which we have rendered a non-distorted image.
/// - At each 'output' pixel coordinate (each run of this shader):
///   - Sub-sample the position
///   - Undistort the position and read the color from the input texture. 
pub const DISTORTION_FRAGMENT_SHADER: &'static str = r#"
#version 330 core

// Input texture which contains non-distorted rendered geometry.
uniform sampler2D u_texture;

// Width/height of u_texture in pixels (doesn't include SSAA scaling)
uniform vec2 u_input_size;

// Optical center of the input texture in pixels.
// (in OpenCV coordinate system)
uniform vec2 u_input_center;

// Output buffer size in pixels.
uniform vec2 u_output_size;

uniform vec2 u_output_center;

// Camera focal length.
uniform vec2 u_focal_length;

uniform float u_k1, u_k2;

uniform int u_supersampling; 

// Current position being rendered in the final image.
// 
// (0,0) is the bottom left of the screen
// (1,1) is the top right of the screen.
in vec2 f_tex_coord;

out vec4 frag_color;

vec2 undistort_point(vec2 distorted_pt) {
    vec2 pt = distorted_pt;

    for (int i = 0; i < 5; i++) {
        float r2 = pt.x * pt.x + pt.y * pt.y;
        float r4 = r2 * r2;
        float k = 1.0 + u_k1 * r2 + u_k2 * r4;        
        pt = distorted_pt / k;
    }

    return pt;
}

void main() {
    vec4 color_sum = vec4(0.0);

    // Size of one screen pixel along both axes.
    vec2 dx = dFdx(f_tex_coord);
    vec2 dy = dFdy(f_tex_coord);
    
    float samples = float(u_supersampling * u_supersampling);
    float step_size = 1.0 / float(u_supersampling);
    
    for (int x = 0; x < u_supersampling; x++) {
        for (int y = 0; y < u_supersampling; y++) {
            
            // Calculate texture coordinate of the current subpixel we are sampling.
            // (with OpenGL axis convention)
            vec2 offset = vec2(float(x) + 0.5, float(y) + 0.5) * step_size - 0.5;
            vec2 sub_coord_gl = f_tex_coord + (dx * offset.x) + (dy * offset.y);
            
            // Flip y to switch to OpenCV style coordinates.
            vec2 sub_coord_cv = vec2(sub_coord_gl.x, 1.0 - sub_coord_gl.y);
            
            // Convert to pixel units
            vec2 pixel_coord = sub_coord_cv * u_output_size;
            
            vec2 normalized_point = (pixel_coord - u_output_center) / u_focal_length;
            
            vec2 undistorted_point = undistort_point(normalized_point);
            
            vec2 input_pixel_coord = undistorted_point * u_focal_length + u_input_center;
            
            vec2 input_coord_cv = input_pixel_coord / u_input_size;
            
            vec2 input_coord_gl = vec2(input_coord_cv.x, 1.0 - input_coord_cv.y);
            
            color_sum += texture(u_texture, input_coord_gl);
        }
    }
    
    frag_color = color_sum / samples;
}

"#;


pub const MATTE_VERTEX_SHADER: &'static str = r#"
#version 330

uniform mat4 u_proj;
uniform mat4 u_modelview;

in vec3 v_position;
in vec3 v_color;

out float f_z;
out vec4 f_color;

void main() {
    f_color = vec4(v_color, 1.0);

    vec4 pos = u_modelview * vec4(v_position, 1.0);

    f_z = length(pos);

	gl_Position = u_proj * pos;
}
"#;


/*
- Higher Z is further away.
- So higher Z should 
*/
pub const MATTE_FRAGMENT_SHADER: &'static str = r#"
#version 330

in float f_z;
in vec4 f_color;

out vec4 frag_color;

void main() {
    float z_far = 5.0;
    float scale = min(max(z_far - f_z, 0.0), z_far) / z_far;

	frag_color = vec4(f_color.x * scale, f_color.y * scale, f_color.z * scale, 1.0);
}
"#;