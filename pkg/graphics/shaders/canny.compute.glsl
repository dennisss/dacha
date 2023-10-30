#version 430 core

layout (local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

layout(rgba8, binding = 0) uniform image2D u_input_image;
layout(rgba8, binding = 1) uniform image2D u_output_image;

const int kernel_size = 5;

// Row-major kernel.
const float kernel[25] = {
    0.0232, 0.0338, 0.0383, 0.0338, 0.0232, 
    0.0338, 0.0492, 0.0558, 0.0492, 0.0338, 
    0.0383, 0.0558, 0.0632, 0.0558, 0.0383, 
    0.0338, 0.0492, 0.0558, 0.0492, 0.0338, 
    0.0232, 0.0338, 0.0383, 0.0338, 0.0232
};

const int kernel_mid = kernel_size / 2;

void main() {
    // Coordinate of the current pixel position at the center of the kernel.
    ivec2 center = ivec2(gl_GlobalInvocationID.xy);

    vec3 value = vec3(0.0, 0.0, 0.0);
    for (int i = 0; i < kernel_size; i += 1) {
        for (int j = 0; j < kernel_size; j += 1) {
            vec4 v = imageLoad(u_input_image, ivec2(center.x + kernel_mid - i, center.y + kernel_mid - j));
            value += v.xyz * kernel[j*kernel_size + i];
        }
    }

    vec4 color = vec4(value, 1.0);

    imageStore(u_output_image, center, color);
}