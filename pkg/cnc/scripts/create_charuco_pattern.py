# Written by Google Gemini
# https://gemini.google.com/app/6711a6b16286a6b0
#
# Actual glass frame size is 179mm x 128mm (slightly over 7 x 5 inches)

# Teslong: 2592 x 1944

# python3 pkg/cnc/scripts/create_charuco_pattern.py
# convert -density 600 calibration_pattern.png checker_board.png charuco_board.pdf
# lpr -o scaling=100 charuco_board.pdf -P Brother_HL_L2460DW_USB
#   TODO: Also need to add high quality settings.

# SCALE_Y = (128.0 / 128.8) # 1.000

# If having issues read
# https://stackoverflow.com/questions/52998331/imagemagick-security-policy-pdf-blocking-conversion

import cv2
import numpy as np

def create_charuco_board():
    # --- Configuration ---
    FILE_NAME = "charuco_board.png"
    DPI = 600
    
    # Paper Dimensions (US Letter)
    PAPER_W_INCH = 8.5
    PAPER_H_INCH = 11
    
    # Backing/Cutout Dimensions
    TARGET_W_INCH = 7.04
    TARGET_H_INCH = 5.04
    
    # Pattern Specs
    SQUARE_SIZE_MM = 5.0
    MARKER_SIZE_MM = 3.8
    INTERNAL_MARGIN_MM = 2.0
    
    # --- SCALING FACTORS ---
    # Adjust these based on your printer's output.
    # Formula: Desired_Length / Measured_Length
    SCALE_X = (165.0 / 164.5) # 1.0000
    SCALE_Y = (115.0 / 114.5) * (128.0 / 128.8) # 1.000
    
    # --- Helpers ---
    def mm_to_px(mm):
        return int(mm * (DPI / 25.4))
        
    def inch_to_px(inch):
        return int(inch * DPI)

    # --- 1. Calculate Grid ---
    target_w_mm = TARGET_W_INCH * 25.4
    target_h_mm = TARGET_H_INCH * 25.4
    
    safe_w_mm = target_w_mm - (INTERNAL_MARGIN_MM * 2)
    safe_h_mm = target_h_mm - (INTERNAL_MARGIN_MM * 2)
    
    squares_x = int(safe_w_mm // SQUARE_SIZE_MM)
    squares_y = int(safe_h_mm // SQUARE_SIZE_MM)
    
    print(f"Grid: {squares_x} cols x {squares_y} rows")
    
    # --- 2. Generate Base Board (Square Pixels) ---
    aruco_dict = cv2.aruco.getPredefinedDictionary(cv2.aruco.DICT_5X5_1000)
    board = cv2.aruco.CharucoBoard(
        (squares_x, squares_y),
        SQUARE_SIZE_MM,
        MARKER_SIZE_MM,
        aruco_dict
    )
    
    # Calculate "perfect" pixel size
    base_w_px = mm_to_px(squares_x * SQUARE_SIZE_MM)
    base_h_px = mm_to_px(squares_y * SQUARE_SIZE_MM)
    
    # Generate standard image
    board_img_raw = board.generateImage((base_w_px, base_h_px), marginSize=0, borderBits=1)
    
    # --- 3. APPLY SCALING (Post-Processing) ---
    scaled_w_px = int(base_w_px * SCALE_X)
    scaled_h_px = int(base_h_px * SCALE_Y)
    
    print(f"Resizing Board: {base_w_px}x{base_h_px} -> {scaled_w_px}x{scaled_h_px}")
    
    # Use INTER_LINEAR for slight stretching
    interp = cv2.INTER_LINEAR if (SCALE_X > 1 or SCALE_Y > 1) else cv2.INTER_AREA
    board_img_scaled = cv2.resize(board_img_raw, (scaled_w_px, scaled_h_px), interpolation=interp)
    
    # --- 4. Create Canvas ---
    paper_w_px = inch_to_px(PAPER_W_INCH)
    paper_h_px = inch_to_px(PAPER_H_INCH)
    canvas = np.ones((paper_h_px, paper_w_px), dtype=np.uint8) * 255
    
    # --- 5. Composite Board ---
    center_x = paper_w_px // 2
    center_y = paper_h_px // 2
    
    x_board_start = center_x - (scaled_w_px // 2)
    y_board_start = center_y - (scaled_h_px // 2)
    
    canvas[y_board_start:y_board_start+scaled_h_px, x_board_start:x_board_start+scaled_w_px] = board_img_scaled
    
    # --- 6. Draw Cut Line (Scaled) ---
    cut_w_px = int(inch_to_px(TARGET_W_INCH) * SCALE_X)
    cut_h_px = int(inch_to_px(TARGET_H_INCH) * SCALE_Y)
    
    x_cut_start = center_x - (cut_w_px // 2)
    y_cut_start = center_y - (cut_h_px // 2)
    x_cut_end = x_cut_start + cut_w_px
    y_cut_end = y_cut_start + cut_h_px
    
    cv2.rectangle(canvas, (x_cut_start, y_cut_start), (x_cut_end, y_cut_end), 150, 4)
    
    # --- 7. Annotations (No Rulers) ---
    font = cv2.FONT_HERSHEY_SIMPLEX
    cv2.putText(canvas, f"Cut: {TARGET_W_INCH}x{TARGET_H_INCH}in", (x_cut_start, y_cut_start - 50), font, 1.2, 150, 3)
    cv2.putText(canvas, f"Scale X:{SCALE_X:.4f} Y:{SCALE_Y:.4f}", (x_cut_start, y_cut_start - 120), font, 1.0, 150, 2)

    cv2.imwrite(FILE_NAME, canvas)
    print(f"Saved {FILE_NAME}")

if __name__ == "__main__":
    create_charuco_board()