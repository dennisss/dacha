import cv2
import numpy as np

def generate_alternating_columns(filename, width=1920, height=1200):
    """
    Generates an image with alternating black and white columns.
    """
    print(f"Generating image: {width}x{height} pixels...")

    # 1. Initialize a completely black image.
    # We use shape (height, width) because NumPy arrays are row-major (y, x).
    # np.uint8 is the standard data type for 8-bit grayscale images.
    img = np.zeros((height, width), dtype=np.uint8)

    # 2. Apply the alternating pattern using NumPy slicing.
    # img[:, 1::2] means:
    #   ':'     -> Select all rows (the entire height of the image)
    #   '1::2'  -> Select columns starting at index 1, and step by 2 (all odd columns)
    # We set these specific columns to 255 (pure white).
    img[:, 1::2] = 255

    # 3. Save the resulting image to disk.
    success = cv2.imwrite(filename, img)
    
    if success:
        print(f"Successfully saved to '{filename}'.")
    else:
        print(f"Failed to save the image. Check your directory permissions.")

if __name__ == "__main__":
    output_filename = "pkg/vision/mocap/scripts/worst_case_image.png"
    generate_alternating_columns(output_filename)