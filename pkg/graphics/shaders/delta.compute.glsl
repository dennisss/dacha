// Compute shader which computes the difference between two images

#version 430 core

layout (local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

layout(rgba8, binding = 0) uniform image2D u_image_a;
layout(rgba8, binding = 1) uniform image2D u_image_b;
layout(binding = 0) uniform atomic_uint u_total_diff;

void main() {
    ivec2 coord = ivec2(gl_GlobalInvocationID.xy);

    vec4 v = imageLoad(u_input_image, ivec2(center.x + kernel_mid - i, center.y + kernel_mid - j));

    vec3 value = vec3(0.0, 0.0, 0.0);
    for (int i = 0; i < kernel_size; i += 1) {
        for (int j = 0; j < kernel_size; j += 1) {

            value += v.xyz * kernel[j*kernel_size + i];
        }
    }

    vec4 color = vec4(value, 1.0);

    imageStore(u_output_image, center, color);
}