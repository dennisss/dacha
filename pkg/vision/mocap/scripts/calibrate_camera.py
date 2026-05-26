import cv2
import numpy as np
import glob
import os

INPUT_DIR = "data/mocap_camera_calib/nsj4t8fr4jzg4"

# Grid Configuration (Number of squares, not corners)
SQUARES_X = 9
SQUARES_Y = 14
SQUARE_SIZE_X_MM = 40.0
SQUARE_SIZE_Y_MM = 40.0

def main():
    # Set up the visualization output directory
    vis_dir = os.path.join(INPUT_DIR, "vis")
    os.makedirs(vis_dir, exist_ok=True)

    # OpenCV checkerboard logic uses internal corners
    pattern_size = (SQUARES_X - 1, SQUARES_Y - 1)
    
    # Pre-compute the 3D coordinates of the checkerboard in real-world space
    objp = np.zeros((pattern_size[0] * pattern_size[1], 3), np.float32)
    
    # Generate the (X, Y) integer grid indices
    grid = np.mgrid[0:pattern_size[0], 0:pattern_size[1]].T.reshape(-1, 2)
    
    # Multiply the integer indices by their respective physical axis scales
    objp[:, :2] = grid * [SQUARE_SIZE_X_MM, SQUARE_SIZE_Y_MM]

    # Arrays to store object points and image points from all valid positions
    objpoints = [] # 3d points in real world space
    imgpoints = [] # 2d points in image plane
    
    # To store image dimensions required by calibrateCamera
    image_size = None 

    mask_saved = False

    # Find all inner directories (ignoring the 'vis' output directory)
    image_files = [f for f in os.listdir(INPUT_DIR) 
                  if f.endswith(".jpg")]

    if not image_files:
        print(f"Error: No images found in {INPUT_DIR}")
        return

    print(f"Found {len(image_files)} images. Starting processing...\n")

    for image_name in sorted(image_files):
        image_path = os.path.join(INPUT_DIR, image_name)

        print(f"Processing '{image_name}'...")

        img = cv2.imread(image_path)
        if image_size is None:
            # OpenCV calibrateCamera requires (width, height)
            image_size = (img.shape[1], img.shape[0]) 


        gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)

        # clahe = cv2.createCLAHE(clipLimit=2.0, tileGridSize=(8, 8))
        # gray = clahe.apply(gray)

        for scale_factor in [1.0, 0.5, 0.25]:
            small_gray = cv2.resize(gray, (0,0), fx=scale_factor, fy=scale_factor, interpolation=cv2.INTER_AREA)

            flags = cv2.CALIB_CB_ADAPTIVE_THRESH | cv2.CALIB_CB_NORMALIZE_IMAGE
            found, corners = cv2.findChessboardCorners(small_gray, pattern_size, flags)

            if found:
                print(f'  -> Found at scale {scale_factor}')
                corners = corners / scale_factor
                break
                
        vis_img = img.copy()

        if found:
            # Sub-pixel refinement for accurate calibration
            criteria = (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 30, 0.001)
            corners_refined = cv2.cornerSubPix(gray, corners, (11, 11), (-1, -1), criteria)
            
            objpoints.append(objp)
            imgpoints.append(corners_refined)

            # Draw corners for the visualization image
            cv2.drawChessboardCorners(vis_img, pattern_size, corners_refined, found)
            print(f"  -> Success: Corners found and refined.")
        else:
            print(f"  -> Failed: Could not detect full pattern.")

        # Save the visualization frame regardless of success (helps debug bad lighting/focus)
        vis_path = os.path.join(vis_dir, f"{image_name}.jpg")
        cv2.imwrite(vis_path, vis_img)

    if len(objpoints) == 0:
        print("\nNo valid checkerboards were found. Cannot run calibration.")
        return

    print(f"\n=========================================")
    print(f"Running Calibration on {len(objpoints)} valid positions...")

    # Flags to restrict calibration parameters:
    # CALIB_ZERO_TANGENT_DIST locks p1 and p2 to 0.0
    # CALIB_FIX_K3 locks the 3rd radial distortion term to 0.0
    # This leaves exactly: fx, fy, cx, cy, k1, k2
    calib_flags = cv2.CALIB_ZERO_TANGENT_DIST | cv2.CALIB_FIX_K3

    ret, mtx, dist, rvecs, tvecs = cv2.calibrateCamera(
        objpoints, imgpoints, image_size, None, None, flags=calib_flags
    )

    # Clean up the display of the distortion array
    # OpenCV outputs a 1x5 array: [k1, k2, p1, p2, k3]
    k1, k2, p1, p2, k3 = dist[0]

    print("=========================================")
    print(f"RMS Reprojection Error: {ret:.4f} pixels")
    print(f"(Lower is better. Usually < 1.0 is acceptable, < 0.5 is great)")
    print("\n--- Camera Matrix (Intrinsics) ---")
    print(f"Focal Length X (fx): {mtx[0, 0]:.2f}")
    print(f"Focal Length Y (fy): {mtx[1, 1]:.2f}")
    print(f"Optical Center X (cx): {mtx[0, 2]:.2f}")
    print(f"Optical Center Y (cy): {mtx[1, 2]:.2f}")
    print("Matrix View:")
    print(np.round(mtx, 2))
    
    print("\n--- Distortion Coefficients ---")
    print(f"k1: {k1:.6f}")
    print(f"k2: {k2:.6f}")
    print(f"p1: {p1:.6f} (Fixed)")
    print(f"p2: {p2:.6f} (Fixed)")
    print(f"k3: {k3:.6f} (Fixed)")
    print("=========================================")

    print(f"focal_length: [{mtx[0, 0]:.2f}, {mtx[1, 1]:.2f}]")
    print(f"center: [{mtx[0, 2]:.2f}, {mtx[1, 2]:.2f}]")
    print(f"k1: {k1:.6f}")
    print(f"k2: {k2:.6f}")


if __name__ == "__main__":
    main()