import math

def calculate_fov(pixel_size_um: float, width_pixels: int, height_pixels: int, focal_length_mm: float) -> dict:
    """
    Calculates the horizontal, vertical, and diagonal Field of View (FOV) of a camera sensor.

    Args:
        pixel_size_um (float): The size of a single pixel in micrometers (um).
        width_pixels (int): The horizontal resolution of the sensor in pixels.
        height_pixels (int): The vertical resolution of the sensor in pixels.
        focal_length_mm (float): The focal length of the lens in millimeters (mm).

    Returns:
        dict: A dictionary containing the horizontal, vertical, and diagonal FOV in degrees.
    """
    # 1. Convert pixel size from micrometers (um) to millimeters (mm)
    pixel_size_mm = pixel_size_um / 1000.0

    # 2. Calculate the physical dimensions of the sensor in mm
    sensor_width_mm = width_pixels * pixel_size_mm
    sensor_height_mm = height_pixels * pixel_size_mm
    
    # Calculate the physical diagonal using the Pythagorean theorem
    sensor_diagonal_mm = math.hypot(sensor_width_mm, sensor_height_mm)

    # 3. Calculate the FOV using the arctan function
    # math.atan returns radians, so we use math.degrees to convert to degrees
    fov_horizontal = 2 * math.degrees(math.atan(sensor_width_mm / (2 * focal_length_mm)))
    fov_vertical = 2 * math.degrees(math.atan(sensor_height_mm / (2 * focal_length_mm)))
    fov_diagonal = 2 * math.degrees(math.atan(sensor_diagonal_mm / (2 * focal_length_mm)))

    return {
        "Horizontal FOV (degrees)": round(fov_horizontal, 2),
        "Vertical FOV (degrees)": round(fov_vertical, 2),
        "Diagonal FOV (degrees)": round(fov_diagonal, 2)
    }

# --- Example Usage using your provided numbers ---
if __name__ == "__main__":
    pixel_size = 3.0      # um
    resolution_w = 1200   # pixels
    resolution_h = 1200   # pixels
    focal_length = 4.35   # mm

    print(f"Sensor Specs: {resolution_w}x{resolution_h} pixels, {pixel_size}um pixel size, {focal_length}mm focal length.\n")
    
    fov_results = calculate_fov(pixel_size, resolution_w, resolution_h, focal_length)
    
    for key, value in fov_results.items():
        print(f"{key}: {value}")