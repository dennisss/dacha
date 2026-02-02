# Written by Google Gemini
# https://gemini.google.com/app/6711a6b16286a6b0

import cv2
import numpy as np
import sys
from concurrent.futures import ProcessPoolExecutor

# --- Configuration ---
INPUT_VIDEO = "input.mp4"       
OUTPUT_VIDEO = "annotated_4k_fast.mp4"
BATCH_SIZE = 30 
DETECTION_SCALE = 1  # 0.5 = Downsample 4K (3840x2160) -> 1080p (1920x1080)

# --- Pattern Dimensions ---
COLS = 34
ROWS = 24
CHECKER_COLS = COLS - 1
CHECKER_ROWS = ROWS - 1

# --- Visual Settings (for 4K output) ---
DOT_RADIUS = 8        # Increased for visibility on 4K
DOT_THICKNESS = -1 
COLORS = [
    (0, 0, 255), (0, 255, 0), (255, 0, 0), (0, 255, 255),
    (255, 0, 255), (255, 255, 0), (255, 128, 0), (128, 255, 0)
]

# --- Helper Functions ---

def draw_colored_points(img, points):
    if points is None: return img
    points = points.reshape(-1, 2)
    for i, pt in enumerate(points):
        color = COLORS[i % len(COLORS)]
        cv2.circle(img, (int(pt[0]), int(pt[1])), DOT_RADIUS, color, DOT_THICKNESS)
        cv2.circle(img, (int(pt[0]), int(pt[1])), DOT_RADIUS + 1, (255, 255, 255), 2)
    return img

def process_single_frame(frame):
    """
    Worker function: 
    1. Downscales frame.
    2. Detects patterns.
    3. Upscales coordinates.
    4. Draws on original 4K frame.
    """
    # 1. Downsample for speed
    small_frame = cv2.resize(frame, None, fx=DETECTION_SCALE, fy=DETECTION_SCALE, interpolation=cv2.INTER_LINEAR)
    gray = cv2.cvtColor(small_frame, cv2.COLOR_BGR2GRAY)
    
    # We draw on the ORIGINAL high-res frame
    annotated = frame 
    
    # Multiplier to map small coordinates back to 4K
    UPSAMPLE_RATIO = 1.0 / DETECTION_SCALE

    # --- 2. Detect ChArUco ---
    aruco_dict = cv2.aruco.getPredefinedDictionary(cv2.aruco.DICT_5X5_1000)
    charuco_board = cv2.aruco.CharucoBoard((COLS, ROWS), 0.005, 0.0038, aruco_dict)
    detector_params = cv2.aruco.DetectorParameters()
    charuco_detector = cv2.aruco.ArucoDetector(aruco_dict, detector_params)
    
    marker_corners, marker_ids, _ = charuco_detector.detectMarkers(gray)
    if marker_ids is not None and len(marker_ids) > 0:
        retval, charuco_corners, charuco_ids = cv2.aruco.interpolateCornersCharuco(
            marker_corners, marker_ids, gray, charuco_board
        )
        if retval > 4 and charuco_ids is not None:
            pairs = sorted(zip(charuco_ids.flatten(), charuco_corners.reshape(-1, 2)), key=lambda x: x[0])
            pts = np.array([p[1] for p in pairs])
            
            # Scale coordinates back up
            pts *= UPSAMPLE_RATIO
            annotated = draw_colored_points(annotated, pts)

    # # --- 3. Detect Checkerboard ---
    # # dims = (CHECKER_COLS, CHECKER_ROWS)
    # dims = (8, 6)
    # ret, corners = cv2.findChessboardCorners(gray, dims, 
    #                                          cv2.CALIB_CB_ADAPTIVE_THRESH + 
    #                                          cv2.CALIB_CB_FAST_CHECK + 
    #                                          cv2.CALIB_CB_NORMALIZE_IMAGE)
    # if ret:
    #     criteria = (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 30, 0.001)
    #     corners2 = cv2.cornerSubPix(gray, corners, (11, 11), (-1, -1), criteria)
        
    #     # Scale coordinates back up
    #     corners2 *= UPSAMPLE_RATIO
    #     annotated = draw_colored_points(annotated, corners2)

    # # --- 4. Detect Asymmetric Circles ---
    # params = cv2.SimpleBlobDetector_Params()
    # params.filterByArea = True
    # # Adjust area filters for the smaller resolution
    # params.minArea = 10   # Smaller because we downscaled
    # params.maxArea = 2500 
    # blob_detector = cv2.SimpleBlobDetector_create(params)
    
    # ret, centers = cv2.findCirclesGrid(gray, (8, 6), # (COLS, ROWS), 
    #                                    cv2.CALIB_CB_ASYMMETRIC_GRID + 
    #                                    cv2.CALIB_CB_CLUSTERING, 
    #                                    blobDetector=blob_detector)
    # if ret:
    #     # Scale coordinates back up
    #     centers *= UPSAMPLE_RATIO
    #     annotated = draw_colored_points(annotated, centers)

    return annotated

def main():
    cap = cv2.VideoCapture(INPUT_VIDEO)
    if not cap.isOpened():
        print(f"Error: Could not open {INPUT_VIDEO}")
        return

    width  = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps    = cap.get(cv2.CAP_PROP_FPS)
    total_frames = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))

    fourcc = cv2.VideoWriter_fourcc(*'mp4v')
    out = cv2.VideoWriter(OUTPUT_VIDEO, fourcc, fps, (width, height))

    print(f"Processing {total_frames} frames (4K Input -> {int(width*DETECTION_SCALE)}x{int(height*DETECTION_SCALE)} Detection)...")
    
    frame_batch = []
    processed_count = 0

    with ProcessPoolExecutor() as executor:
        while True:
            ret, frame = cap.read()
            if ret:
                frame_batch.append(frame)
            
            if len(frame_batch) == BATCH_SIZE or (not ret and frame_batch):
                results = executor.map(process_single_frame, frame_batch)
                
                for res_frame in results:
                    out.write(res_frame)
                    processed_count += 1
                    print(f"Processed {processed_count}/{total_frames}...", end='\r')
                
                frame_batch = []

            if not ret:
                break

    cap.release()
    out.release()
    print(f"\nDone! Saved to {OUTPUT_VIDEO}")

if __name__ == "__main__":
    main()