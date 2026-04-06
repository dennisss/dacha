import argparse
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
from moviepy import ImageSequenceClip
import os
import tempfile
from multiprocessing import Pool
from tqdm import tqdm

# Settings
FPS = 30
WIDTH = 1200
HEIGHT = 1200
DPI = 100

def get_rotation_matrix(u, v):
    """Return rotation matrix that rotates u to v."""
    u = u / np.linalg.norm(u)
    v = v / np.linalg.norm(v)
    axis = np.cross(u, v)
    s = np.linalg.norm(axis)
    c = np.dot(u, v)
    if s < 1e-6:
        if c > 0:
            return np.eye(3)
        else:
            return -np.eye(3)
    Vx = np.array([
        [0, -axis[2], axis[1]],
        [axis[2], 0, -axis[0]],
        [-axis[1], axis[0], 0]
    ])
    R = np.eye(3) + Vx + (Vx @ Vx) * ((1 - c) / (s ** 2))
    return R

def project_3d_to_2d(points, azim=45, elev=30):
    """Project 3D points to 2D using orthographic projection."""
    azim = np.radians(azim)
    elev = np.radians(elev)
    
    x = points[:, 0]
    y = points[:, 1]
    z = points[:, 2]
    
    x_scr = x * np.cos(azim) - y * np.sin(azim)
    y_scr = -(x * np.sin(azim) * np.sin(elev) + y * np.cos(azim) * np.sin(elev)) + z * np.cos(elev)
    
    return np.column_stack((x_scr, y_scr))

def _worker_plot_frame(args):
    idx = args['idx']
    x, y, z = args['data']
    frame_path = args['frame_path']
    
    G = np.array([x, y, z])
    norm = np.linalg.norm(G)
    if norm < 1e-6:
        G = np.array([0.,0.,1.])
    else:
        G = G / norm
        
    R = get_rotation_matrix(np.array([0, 0, 1]), G)
    
    X_axis = R @ np.array([1, 0, 0])
    Y_axis = R @ np.array([0, 1, 0])
    Z_axis = R @ np.array([0, 0, 1])

    fig = plt.figure(figsize=(WIDTH/DPI, HEIGHT/DPI), dpi=DPI)
    fig.patch.set_facecolor('black')
    
    # Create 2D axes taking up the whole figure
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_facecolor('black')
    ax.axis('off')

    O = np.array([0, 0, 0])
    points_3d = np.array([O, X_axis, Y_axis, Z_axis])
    points_2d = project_3d_to_2d(points_3d, azim=45, elev=30)
    
    O_2d = points_2d[0]
    X_2d = points_2d[1]
    Y_2d = points_2d[2]
    Z_2d = points_2d[3]

    ax.plot([O_2d[0], X_2d[0]], [O_2d[1], X_2d[1]], color='red', linewidth=7)
    ax.plot([O_2d[0], Y_2d[0]], [O_2d[1], Y_2d[1]], color='green', linewidth=7)
    ax.plot([O_2d[0], Z_2d[0]], [O_2d[1], Z_2d[1]], color='blue', linewidth=7)

    # Keep limits fixed so length doesn't scale
    ax.set_xlim([-1.2, 1.2])
    ax.set_ylim([-1.2, 1.2])
    ax.set_aspect('equal')
    
    plt.savefig(frame_path, facecolor='black')
    plt.close(fig)
    return frame_path

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--input', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()

    df = pd.read_csv(args.input)
    
    start_time = df['time'].iloc[0]
    end_time = df['time'].iloc[-1]
    
    video_duration = end_time - start_time
    num_frames = int(np.ceil(video_duration * FPS))
    frame_times = start_time + np.arange(num_frames) / float(FPS)
    
    times = df['time'].values
    indices = np.searchsorted(times, frame_times)
    indices = np.clip(indices, 1, len(times) - 1)
    
    left_diff = frame_times - times[indices - 1]
    right_diff = times[indices] - frame_times
    nearest_indices = np.where(left_diff < right_diff, indices - 1, indices)
    
    with tempfile.TemporaryDirectory() as temp_dir:
        tasks = []
        for i, idx in enumerate(nearest_indices):
            row = df.iloc[idx]
            tasks.append({
                'idx': i,
                'data': (row['x'], row['y'], row['z']),
                'frame_path': os.path.join(temp_dir, f"frame_{i:06d}.png")
            })
            
        print(f"Rendering {len(tasks)} frames...")
        frame_files = []
        with Pool(processes=max(1, os.cpu_count() // 2)) as pool:
            for saved_path in tqdm(pool.imap(_worker_plot_frame, tasks), total=len(tasks)):
                frame_files.append(saved_path)
                
        print(f"Encoding video to {args.output}...")
        clip = ImageSequenceClip(frame_files, fps=FPS)
        clip.write_videofile(args.output, codec='libx264')

if __name__ == '__main__':
    main()
