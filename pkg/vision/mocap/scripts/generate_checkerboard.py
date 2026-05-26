# python3 pkg/vision/mocap/scripts/generate_checkerboard.py

# TODO: Currently it is 600 DPI output but IIUC, PrintPapa uses 150 DPI so improve the output
# so that downscaling hits 150 DPI boundaries better.

import cv2
import numpy as np
from PIL import Image

# PrintPapa Large Format Requirements
DPI = 600
BLEED_IN = 0.25  # 1/4 inch bleed region on all sides

# Target Paper Size (Example: 11x17 Tabloid/Ledger)
PAPER_WIDTH_IN = 16.5
PAPER_HEIGHT_IN = 24

# Pattern Physical Dimensions
SQUARE_SIZE_MM = 40
MARKER_SIZE_MM = 15
MIN_MARGIN_MM = SQUARE_SIZE_MM / 2

# ArUco Configuration (Using 250 to support massive grids without crashing)
ARUCO_DICT = cv2.aruco.DICT_4X4_250
# ==========================================

def _prepare_canvas_and_grid():
    """
    Calculates dynamic grid sizes and exact pixel offsets to center the pattern.
    Accounts for physical print bleed by expanding the final canvas size, while
    restricting the grid drawing area to the true paper dimensions.
    """
    total_width_in = PAPER_WIDTH_IN + (2 * BLEED_IN)
    total_height_in = PAPER_HEIGHT_IN + (2 * BLEED_IN)
    
    total_width_px = int(total_width_in * DPI)
    total_height_px = int(total_height_in * DPI)
    
    square_px = int((SQUARE_SIZE_MM / 25.4) * DPI)
    min_margin_px = int((MIN_MARGIN_MM / 25.4) * DPI)
    
    avail_width_px = int(PAPER_WIDTH_IN * DPI) - (2 * min_margin_px)
    avail_height_px = int(PAPER_HEIGHT_IN * DPI) - (2 * min_margin_px)

    squares_x = avail_width_px // square_px
    squares_y = avail_height_px // square_px

    board_width_px = squares_x * square_px
    board_height_px = squares_y * square_px

    offset_x = (total_width_px - board_width_px) // 2
    offset_y = (total_height_px - board_height_px) // 2

    canvas = np.ones((total_height_px, total_width_px), dtype=np.uint8) * 255

    return canvas, squares_x, squares_y, square_px, offset_x, offset_y, board_width_px, board_height_px

def _save_as_pdf(canvas, filename):
    """
    Converts the OpenCV NumPy array to a PIL Image and saves it 
    directly as a PDF with the correct resolution embedded.
    """
    # Convert OpenCV BGR/Grayscale to PIL Image
    pil_img = Image.fromarray(canvas)
    
    # Save directly as PDF, enforcing the physical DPI
    pil_img.save(filename, "PDF", resolution=DPI)
    print(f"Saved directly to PDF: {filename}")


def generate_checkerboard():
    canvas, squares_x, squares_y, square_px, offset_x, offset_y, _, _ = _prepare_canvas_and_grid()

    for r in range(squares_y):
        for c in range(squares_x):
            if (r + c) % 2 == 0:
                top_left = (offset_x + c * square_px, offset_y + r * square_px)
                bottom_right = (offset_x + (c + 1) * square_px, offset_y + (r + 1) * square_px)
                cv2.rectangle(canvas, top_left, bottom_right, 0, -1)

    filename = f"data/calib/checkerboard_{SQUARE_SIZE_MM}mm_{squares_x}x{squares_y}_printpapa.pdf"
    _save_as_pdf(canvas, filename)
    print(f"Grid: {squares_x}x{squares_y} squares | {squares_x-1}x{squares_y-1} internal corners")


def generate_charuco_board():
    canvas, squares_x, squares_y, _, offset_x, offset_y, board_w_px, board_h_px = _prepare_canvas_and_grid()

    aruco_dict = cv2.aruco.getPredefinedDictionary(ARUCO_DICT)
    board = cv2.aruco.CharucoBoard(
        (squares_x, squares_y), 
        SQUARE_SIZE_MM / 1000.0,   
        MARKER_SIZE_MM / 1000.0, 
        aruco_dict
    )

    board_img = board.generateImage((board_w_px, board_h_px))
    canvas[offset_y:offset_y+board_h_px, offset_x:offset_x+board_w_px] = board_img

    filename = f"data/calib/charuco_{SQUARE_SIZE_MM}mm_{squares_x}x{squares_y}_printpapa.pdf"
    _save_as_pdf(canvas, filename)
    print(f"Grid: {squares_x}x{squares_y} squares | {squares_x-1}x{squares_y-1} internal corners")


if __name__ == "__main__":
    print(f"Paper Size: {PAPER_WIDTH_IN}\" x {PAPER_HEIGHT_IN}\"")
    print(f"Canvas Size (with {BLEED_IN}\" bleed): {PAPER_WIDTH_IN + 2*BLEED_IN}\" x {PAPER_HEIGHT_IN + 2*BLEED_IN}\"")
    print(f"Resolution: {DPI} DPI\n")
    
    print("Generating Standard Checkerboard...")
    generate_checkerboard()
    
    # print("\nGenerating ChArUco Board...")
    # generate_charuco_board()