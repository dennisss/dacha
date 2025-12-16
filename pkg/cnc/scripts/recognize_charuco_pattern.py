# Written by Google Gemini
# https://gemini.google.com/app/6711a6b16286a6b0

import cv2
import numpy as np
import glob
import os
import csv
import math
import random
from concurrent.futures import ProcessPoolExecutor, as_completed

# --- Configuration ---
INPUT_FOLDER = 'data/skew'
OUTPUT_FOLDER = 'data/skew_out'
CSV_FILENAME = 'camera_poses.csv'
CALIB_FILENAME = 'calibration_data.npz'
SAVE_DEBUG_EVERY_N = 10
CALIBRATION_SAMPLE_SIZE = 200  # Max images to use for calibration

# Board Settings
SQUARES_X = 34
SQUARES_Y = 24
SQUARE_LENGTH_MM = 5.0
MARKER_LENGTH_MM = 3.8
ARUCO_DICT_ID = cv2.aruco.DICT_5X5_1000

# --- Worker Function ---
def process_image(args):
    """
    Independent worker function to detect markers.
    Args: (index, img_path)
    """
    index, img_path = args
    try:
        aruco_dict = cv2.aruco.getPredefinedDictionary(ARUCO_DICT_ID)
        board = cv2.aruco.CharucoBoard(
            (SQUARES_X, SQUARES_Y), 
            SQUARE_LENGTH_MM, 
            MARKER_LENGTH_MM, 
            aruco_dict
        )
        detector_params = cv2.aruco.DetectorParameters()
        detector = cv2.aruco.ArucoDetector(aruco_dict, detector_params)

        img = cv2.imread(img_path)
        if img is None: return None

        gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
        image_size = gray.shape[::-1]
        
        marker_corners, marker_ids, _ = detector.detectMarkers(gray)
        
        result = None
        if marker_ids is not None and len(marker_ids) > 0:
            retval, charuco_corners, charuco_ids = cv2.aruco.interpolateCornersCharuco(
                marker_corners, marker_ids, gray, board
            )
            
            if retval > 6:
                if index % SAVE_DEBUG_EVERY_N == 0:
                    debug_img = cv2.aruco.drawDetectedCornersCharuco(
                        img.copy(), charuco_corners, charuco_ids, (0, 255, 0)
                    )
                    cv2.putText(debug_img, f"ID: {index}", (50, 50), cv2.FONT_HERSHEY_SIMPLEX, 1, (0,0,255), 2)
                    base_name = os.path.basename(img_path)
                    cv2.imwrite(os.path.join(OUTPUT_FOLDER, f"debug_{base_name}"), debug_img)

                result = {
                    'path': img_path,
                    'corners': charuco_corners,
                    'ids': charuco_ids,
                    'size': image_size
                }
        return result
    except Exception as e:
        print(f"\nError processing {img_path}: {e}")
        return None

# --- Safety Check ---
def validate_calibration(camera_matrix, image_size):
    fx = camera_matrix[0, 0]
    fy = camera_matrix[1, 1]
    cx = camera_matrix[0, 2]
    cy = camera_matrix[1, 2]
    width, height = image_size

    print("\n--- Calibration Safety Check ---")
    fov_x_deg = 2 * math.atan(width / (2 * fx)) * (180 / math.pi)
    print(f"  > Calculated Horizontal FOV: {fov_x_deg:.1f}°")
    
    if fov_x_deg < 40:
        print("  [FAIL] FOV too narrow (Telephoto hallucination).")
        return False
    elif fov_x_deg > 160:
        print("  [FAIL] FOV too wide (Fisheye hallucination).")
        return False
    
    aspect_ratio = fx / fy
    if not (0.95 < aspect_ratio < 1.05):
        print(f"  [FAIL] Non-square pixels (Ratio: {aspect_ratio:.3f})")
        return False

    max_drift = max(width, height) * 0.1
    center_drift = max(abs(cx - width/2), abs(cy - height/2))
    if center_drift > max_drift:
        print(f"  [FAIL] Optical center drift too high.")
        return False

    print("  [PASS] Calibration looks physically plausible.")
    print("--------------------------------")
    return True

# --- Main ---
def main():
    if not os.path.exists(OUTPUT_FOLDER): os.makedirs(OUTPUT_FOLDER)

    image_files = sorted(glob.glob(os.path.join(INPUT_FOLDER, '*')))
    valid_exts = ('.jpg', '.jpeg', '.png', '.bmp')
    image_files = [f for f in image_files if f.lower().endswith(valid_exts)]
    
    if not image_files:
        print("No images found.")
        return

    print(f"Found {len(image_files)} images. Starting parallel detection...")

    # 1. Parallel Detection (ALL Images)
    all_results = []
    image_size = None
    
    task_args = [(i, f) for i, f in enumerate(image_files)]
    
    with ProcessPoolExecutor() as executor:
        futures = [executor.submit(process_image, arg) for arg in task_args]
        for i, future in enumerate(as_completed(futures)):
            res = future.result()
            print(f"Progress: [{i+1}/{len(image_files)}]...", end='\r')
            if res is not None:
                all_results.append(res)
                if image_size is None: image_size = res['size']
                
    print(f"\n\nDetection Complete. Found patterns in {len(all_results)} images.")
    if len(all_results) < 1: return

    # 2. Prepare Data for Calibration (Random Subset)
    # Extract lists from results
    all_corners = [r['corners'] for r in all_results]
    all_ids = [r['ids'] for r in all_results]
    
    calib_corners = all_corners
    calib_ids = all_ids
    
    if len(all_results) > CALIBRATION_SAMPLE_SIZE:
        print(f"\nSubsampling: Selecting random {CALIBRATION_SAMPLE_SIZE} images for calibration...")
        # Get random indices
        indices = random.sample(range(len(all_results)), CALIBRATION_SAMPLE_SIZE)
        calib_corners = [all_corners[i] for i in indices]
        calib_ids = [all_ids[i] for i in indices]
    else:
        print(f"\nUsing all {len(all_results)} images for calibration.")

    # 3. Run Calibration
    print("Running Calibration...")
    aruco_dict = cv2.aruco.getPredefinedDictionary(ARUCO_DICT_ID)
    board = cv2.aruco.CharucoBoard((SQUARES_X, SQUARES_Y), SQUARE_LENGTH_MM, MARKER_LENGTH_MM, aruco_dict)

    ret, camera_matrix, dist_coeffs, rvecs_calib, tvecs_calib = cv2.aruco.calibrateCameraCharuco(
        calib_corners, calib_ids, board, image_size, None, None
    )
    
    print(f"Reprojection Error: {ret:.4f}")
    if not validate_calibration(camera_matrix, image_size):
        print("WARNING: Safety checks failed.")

    np.savez(CALIB_FILENAME, mtx=camera_matrix, dist=dist_coeffs, err=ret)
    
    # 4. Estimate Pose for ALL Images (using new matrix)
    print("\nEstimating poses for ALL detected images (Sorted by Filename)...")
    
    all_results.sort(key=lambda x: x['path'])
    
    fx = camera_matrix[0, 0]
    fy = camera_matrix[1, 1]
    avg_focal_length_px = (fx + fy) / 2

    # --- Calculate Diagonal FOV ---
    width, height = image_size
    diagonal_px = math.sqrt(width**2 + height**2)
    fov_diagonal_deg = 2 * math.atan(diagonal_px / (2 * avg_focal_length_px)) * (180 / math.pi)

    with open(CSV_FILENAME, 'w', newline='') as csvfile:
        csv_writer = csv.writer(csvfile)
        csv_writer.writerow([
            'Image_Name', 'Res_mm_per_px', 'Distance_to_Board_mm',
            'Cam_X_mm', 'Cam_Y_mm', 'Cam_Z_mm',
            'tvec_x', 'tvec_y', 'tvec_z',
            'rvec_x', 'rvec_y', 'rvec_z'
        ])
        
        total_res = 0
        
        for res in all_results:
            corners = res['corners']
            ids = res['ids']
            file_name = os.path.basename(res['path'])
            
            retval, rvec, tvec = cv2.aruco.estimatePoseCharucoBoard(
                corners, ids, board, camera_matrix, dist_coeffs, None, None
            )
            
            if retval:
                R, _ = cv2.Rodrigues(rvec)
                cam_pos = -np.dot(R.T, tvec).flatten()
                
                z_depth_mm = tvec[2][0]
                mm_per_px = z_depth_mm / avg_focal_length_px
                total_res += mm_per_px
                dist_mm = np.linalg.norm(tvec)

                csv_writer.writerow([
                    file_name,
                    f"{mm_per_px:.4f}",
                    f"{dist_mm:.2f}",
                    f"{cam_pos[0]:.2f}", f"{cam_pos[1]:.2f}", f"{cam_pos[2]:.2f}",
                    f"{tvec[0][0]:.4f}", f"{tvec[1][0]:.4f}", f"{tvec[2][0]:.4f}",
                    f"{rvec[0][0]:.4f}", f"{rvec[1][0]:.4f}", f"{rvec[2][0]:.4f}"
                ])

    # --- FINAL OUTPUT ---
    print("\n" + "="*40)
    print(f"FINAL CALIBRATION REPORT")
    print(f"{'='*40}")
    print(f"Image Resolution:      {width} x {height}")
    print(f"Avg Focal Length:      {avg_focal_length_px:.2f} px")
    print(f"Diagonal FOV:          {fov_diagonal_deg:.2f} degrees")
    print(f"Avg Resolution (scale):{(total_res / len(all_results)):.4f} mm/pixel")
    print(f"Data saved to:         {CSV_FILENAME}")
    print(f"{'='*40}")

if __name__ == "__main__":
    main()