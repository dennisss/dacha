import math

def calculate_camera_specs(pixel_size_um: float, width_pixels: int, height_pixels: int, focal_length_mm: float, object_distance_m: float = None) -> dict:
    """
    Calculates the FOV of a camera sensor and optionally the spatial resolution of a single pixel.

    Args:
        pixel_size_um (float): The size of a single pixel in micrometers (um).
        width_pixels (int): The horizontal resolution of the sensor in pixels.
        height_pixels (int): The vertical resolution of the sensor in pixels.
        focal_length_mm (float): The focal length of the lens in millimeters (mm).
        object_distance_m (float, optional): Distance to the target object in meters (m).

    Returns:
        dict: A dictionary containing the calculated FOVs and spatial resolution.
    """
    # 1. Convert pixel size from micrometers (um) to millimeters (mm)
    pixel_size_mm = pixel_size_um / 1000.0

    # 2. Calculate the physical dimensions of the sensor in mm
    sensor_width_mm = width_pixels * pixel_size_mm
    sensor_height_mm = height_pixels * pixel_size_mm
    sensor_diagonal_mm = math.hypot(sensor_width_mm, sensor_height_mm)

    # 3. Calculate the FOVs using the arctan function
    fov_horizontal = 2 * math.degrees(math.atan(sensor_width_mm / (2 * focal_length_mm)))
    fov_vertical = 2 * math.degrees(math.atan(sensor_height_mm / (2 * focal_length_mm)))
    fov_diagonal = 2 * math.degrees(math.atan(sensor_diagonal_mm / (2 * focal_length_mm)))

    # Dictionary to store the results
    results = {
        "Horizontal FOV (degrees)": round(fov_horizontal, 2),
        "Vertical FOV (degrees)": round(fov_vertical, 2),
        "Diagonal FOV (degrees)": round(fov_diagonal, 2)
    }

    # 4. Calculate spatial resolution if distance is provided
    if object_distance_m is not None:
        # Convert distance from meters to millimeters
        distance_mm = object_distance_m * 1000.0
        
        # Calculate real-world pixel resolution
        pixel_resolution_mm = (pixel_size_mm * distance_mm) / focal_length_mm
        
        # Add to results
        results[f"Single Pixel Resolution at {object_distance_m}m (mm/pixel)"] = round(pixel_resolution_mm, 2)

    return results

if __name__ == "__main__":
    pixel_size = 3.0      # um
    resolution_w = 1920   #  pixels
    resolution_h = 1200   #  pixels
    focal_length = 4.35   # mm
    target_distance = 2 # meters

    print(f"Sensor Specs: {resolution_w}x{resolution_h} pixels, {pixel_size}um pixel size, {focal_length}mm focal length.")
    print(f"Target Distance: {target_distance} meters.\n")
    
    specs_results = calculate_camera_specs(pixel_size, resolution_w, resolution_h, focal_length, target_distance)
    
    for key, value in specs_results.items():
        print(f"{key}: {value}")